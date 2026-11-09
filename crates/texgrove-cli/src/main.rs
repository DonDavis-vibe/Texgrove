//! `texgrove`: Texgrove's kernel on the command line, for checking meshes and trying things
//! without Meshgrove.

use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;
use texgrove_core::bvh::Bvh;
use texgrove_core::displace::Relief;
use texgrove_core::export::{stl, threemf};
use texgrove_core::mask::{EXCLUDED, Mask};
use texgrove_core::pattern::Pattern;
use texgrove_core::pipeline::{self, Settings};
use texgrove_core::project::{Mapping, Placement};
use texgrove_core::quality::{self, Quality};
use texgrove_core::select;
use texgrove_core::topology::Topology;
use texgrove_core::{Mesh, diagnostics, import, weld};

#[derive(Parser)]
#[command(name = "texgrove", version, about = "Displacement textures for 3D-print meshes")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Say whether a mesh is closed, in one piece and facing outwards.
    Diag {
        /// An STL file.
        file: PathBuf,
    },
    /// Read a mesh, join its vertices and write it as .stl or .3mf.
    Convert {
        /// An STL file.
        input: PathBuf,
        /// Where to write; the ending picks the format (.stl or .3mf).
        output: PathBuf,
    },
    /// Put a texture onto a mesh.
    Apply(Apply),
    /// List the built-in patterns, or write one as a greyscale picture (.pgm).
    Pattern {
        /// The pattern to write; without it, the names are listed.
        name: Option<String>,
        /// Where to write the picture.
        #[arg(short, long, default_value = "pattern.pgm")]
        output: PathBuf,
        /// Cells along the picture's width.
        #[arg(long, default_value_t = 512)]
        size: u32,
    },
}

#[derive(clap::Args)]
struct Apply {
    /// An STL file.
    input: PathBuf,
    /// Where to write; the ending picks the format (.stl or .3mf).
    #[arg(short, long)]
    output: PathBuf,
    /// A built-in pattern (see `texgrove pattern`).
    #[arg(short, long, default_value = "knurl")]
    texture: String,
    /// The width of one tile in mm.
    #[arg(long, default_value_t = 5.0)]
    tile: f32,
    /// From the texture's lowest to its highest point, in mm.
    #[arg(long, default_value_t = 0.5)]
    depth: f32,
    /// The texture height that leaves the surface in place: 0.5 raises and sinks, 0 only raises.
    #[arg(long, default_value_t = 0.5)]
    midlevel: f32,
    /// Swap high and low.
    #[arg(long)]
    invert: bool,
    /// How the texture is laid on.
    #[arg(long, value_enum, default_value_t = MappingArg::Triplanar)]
    mapping: MappingArg,
    /// Turn the texture, in degrees.
    #[arg(long, default_value_t = 0.0)]
    rotation: f32,
    /// How fine to work: the mesh's edge length and size follow from it, the texture and the nozzle.
    #[arg(long, value_enum, default_value_t = QualityArg::Normal)]
    quality: QualityArg,
    /// The printer's nozzle width in mm.
    #[arg(long, default_value_t = 0.4)]
    nozzle: f32,
    /// Where the texture goes: everywhere, only on what lies open to the outside, or only inside.
    #[arg(long = "where", value_enum, default_value_t = WhereArg::All)]
    place: WhereArg,
    /// Leave out faces looking down within this many degrees (what lies on the bed); 0 for none.
    #[arg(long, default_value_t = 5.0)]
    bottom_angle: f32,
    /// Leave out faces looking up within this many degrees; 0 for none.
    #[arg(long, default_value_t = 0.0)]
    top_angle: f32,
    /// Let the texture run out over this many mm towards everything that stays in place.
    #[arg(long, default_value_t = 0.0)]
    falloff: f32,
    /// Instead of the quality's choice: the longest edge of the refined mesh in mm.
    #[arg(long)]
    edge: Option<f32>,
    /// Instead of the quality's choice: the most triangles the result may have; 0 keeps them all.
    #[arg(long)]
    triangles: Option<usize>,
    /// Instead of the quality's choice: below the budget, take away what moves the surface by
    /// less than this many mm.
    #[arg(long)]
    tolerance: Option<f32>,
    /// Let the model's sharp edges move with the texture.
    #[arg(long)]
    free_edges: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum QualityArg {
    Draft,
    Normal,
    Fine,
}

#[derive(Clone, Copy, PartialEq, ValueEnum)]
enum WhereArg {
    All,
    Outside,
    Inside,
}

#[derive(Clone, Copy, ValueEnum)]
enum MappingArg {
    Triplanar,
    Cubic,
    Cylindrical,
    Outline,
    Spherical,
    Xy,
    Xz,
    Yz,
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("texgrove: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Diag { file } => {
            let welded = load(&file)?;
            println!("{}", file.display());
            print_report(&diagnostics::diagnose(&welded.mesh), welded.collapsed);
            Ok(())
        }
        Command::Convert { input, output } => {
            let mesh = load(&input)?.mesh;
            save(&output, &mesh)
        }
        Command::Apply(apply) => {
            let pattern = Pattern::from_id(&apply.texture)
                .ok_or_else(|| format!("no pattern \"{}\"; there are: {}", apply.texture, pattern_names()))?;
            let source = load(&apply.input)?.mesh;
            let map = pattern.render(1024);
            let quality = match apply.quality {
                QualityArg::Draft => Quality::Draft,
                QualityArg::Normal => Quality::Normal,
                QualityArg::Fine => Quality::Fine,
            };
            let mut finish = quality::finish(quality, apply.tile * pattern.detail(), apply.nozzle);
            if let Some(edge) = apply.edge {
                finish.max_edge = edge;
            }
            if let Some(triangles) = apply.triangles {
                finish.triangles = if triangles == 0 { usize::MAX } else { triangles };
            }
            if let Some(tolerance) = apply.tolerance {
                finish.tolerance = tolerance;
            }
            finish.protect_edges = !apply.free_edges;
            let mut mask = Mask {
                top_degrees: apply.top_angle,
                bottom_degrees: apply.bottom_angle,
                falloff: apply.falloff,
                ..Mask::default()
            };
            if apply.place != WhereArg::All {
                let inside = select::inside(&source, &Topology::build(&source), &Bvh::build(&source), 30.0);
                let wanted = apply.place == WhereArg::Inside;
                mask.faces = inside.iter().map(|inside| if *inside == wanted { 0 } else { EXCLUDED }).collect();
                let kept = mask.faces.iter().filter(|mark| **mark == 0).count();
                println!("  textured faces   {kept} of {} ({})", source.triangles.len(), apply.place_name());
            }
            let settings = Settings {
                mapping: match apply.mapping {
                    MappingArg::Triplanar => Mapping::default(),
                    MappingArg::Cubic => Mapping::Cubic { blend: 0.2 },
                    MappingArg::Cylindrical => Mapping::Cylindrical { cap_degrees: 30.0, cap_blend: 10.0 },
                    MappingArg::Outline => Mapping::Outline { cap_degrees: 30.0, cap_blend: 10.0 },
                    MappingArg::Spherical => Mapping::Spherical,
                    MappingArg::Xy => Mapping::PlanarXY,
                    MappingArg::Xz => Mapping::PlanarXZ,
                    MappingArg::Yz => Mapping::PlanarYZ,
                },
                placement: Placement { tile: apply.tile, rotation: apply.rotation, ..Placement::default() },
                relief: Relief { depth: apply.depth, midlevel: apply.midlevel, invert: apply.invert },
                mask,
                finish,
            };
            let started = Instant::now();
            let textured = pipeline::texture(&source, &map, &settings);
            let took = started.elapsed();
            if textured.edge > settings.finish.max_edge {
                println!(
                    "  (edges of {:.2} mm were asked for; that would have been too many triangles)",
                    settings.finish.max_edge
                );
            }
            println!("  refined to       {} triangles, {:.2} mm edges", textured.refined_triangles, textured.edge);
            print_report(&diagnostics::diagnose(&textured.mesh), 0);
            println!("  textured in      {:.2} s", took.as_secs_f64());
            save(&apply.output, &textured.mesh)
        }
        Command::Pattern { name: None, .. } => {
            println!("{}", pattern_names());
            Ok(())
        }
        Command::Pattern { name: Some(name), output, size } => {
            let pattern = Pattern::from_id(&name)
                .ok_or_else(|| format!("no pattern \"{name}\"; there are: {}", pattern_names()))?;
            let map = pattern.render(size.clamp(2, 8192));
            let mut bytes = format!("P5\n{} {}\n255\n", map.width(), map.height()).into_bytes();
            bytes.extend(map.to_grey());
            std::fs::write(&output, &bytes).map_err(|e| format!("cannot write {}: {e}", output.display()))?;
            println!("{}: {} × {}", output.display(), map.width(), map.height());
            Ok(())
        }
    }
}

impl Apply {
    fn place_name(&self) -> &'static str {
        match self.place {
            WhereArg::All => "everywhere",
            WhereArg::Outside => "outside only",
            WhereArg::Inside => "inside only",
        }
    }
}

fn pattern_names() -> String {
    Pattern::ALL.map(Pattern::id).join(", ")
}

fn print_report(report: &diagnostics::Report, collapsed: usize) {
    let size = report.bounds.size();
    println!("  size             {:.2} × {:.2} × {:.2} mm", size[0], size[1], size[2]);
    println!("  triangles        {}", report.triangles);
    println!("  vertices         {}", report.vertices);
    println!("  shells           {}", report.shells);
    println!("  open edges       {}", report.open_edges);
    println!("  non-manifold     {}", report.non_manifold_edges);
    println!("  facing both ways {}", report.disagreeing_edges);
    println!("  flat triangles   {}", report.flat_triangles + collapsed);
    println!("  surface          {:.1} mm²", report.area);
    println!("  volume           {:.1} mm³", report.volume);
    println!("  verdict          {}", verdict(report));
}

fn verdict(report: &diagnostics::Report) -> &'static str {
    if report.sound() {
        "sound: closed and facing outwards"
    } else if report.closed() && report.disagreeing_edges == 0 {
        "closed, but inside out"
    } else if report.closed() {
        "closed, but its triangles do not agree on the outside"
    } else {
        "not closed"
    }
}

fn ending(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase()
}

/// Reads a mesh file and joins its vertices.
fn load(path: &Path) -> Result<weld::Welded, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let soup: Mesh = match ending(path).as_str() {
        "stl" => import::stl::read(&bytes).map_err(|e| format!("{}: {e}", path.display()))?,
        other => return Err(format!("cannot read .{other} files yet, only .stl")),
    };
    Ok(weld::weld_default(&soup))
}

/// Writes a mesh as the file's ending asks.
fn save(path: &Path, mesh: &Mesh) -> Result<(), String> {
    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("model").to_owned();
    let bytes = match ending(path).as_str() {
        "stl" => stl::to_bytes(mesh, &name),
        "3mf" => {
            let object = threemf::Object { name: &name, mesh, transform: None };
            threemf::to_bytes(&name, &[object]).map_err(|e| format!("cannot pack the 3MF: {e}"))?
        }
        other => return Err(format!("cannot write .{other} files, only .stl and .3mf")),
    };
    std::fs::write(path, &bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    println!("{}: {} triangles, {} bytes", path.display(), mesh.triangles.len(), bytes.len());
    Ok(())
}
