//! `texgrove`: Texgrove's kernel on the command line, for checking meshes and trying things
//! without Meshgrove.

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use texgrove_core::export::{stl, threemf};
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
            let report = diagnostics::diagnose(&welded.mesh);
            let size = report.bounds.size();
            println!("{}", file.display());
            println!("  size             {:.2} × {:.2} × {:.2} mm", size[0], size[1], size[2]);
            println!("  triangles        {}", report.triangles);
            println!("  vertices         {}", report.vertices);
            println!("  shells           {}", report.shells);
            println!("  open edges       {}", report.open_edges);
            println!("  non-manifold     {}", report.non_manifold_edges);
            println!("  facing both ways {}", report.disagreeing_edges);
            println!("  flat triangles   {}", report.flat_triangles + welded.collapsed);
            println!("  surface          {:.1} mm²", report.area);
            println!("  volume           {:.1} mm³", report.volume);
            println!("  verdict          {}", verdict(&report));
            Ok(())
        }
        Command::Convert { input, output } => {
            let mesh = load(&input)?.mesh;
            let name = output.file_stem().and_then(|s| s.to_str()).unwrap_or("model").to_owned();
            let bytes = match ending(&output).as_str() {
                "stl" => stl::to_bytes(&mesh, &name),
                "3mf" => {
                    let object = threemf::Object { name: &name, mesh: &mesh, transform: None };
                    threemf::to_bytes(&name, &[object]).map_err(|e| format!("cannot pack the 3MF: {e}"))?
                }
                other => return Err(format!("cannot write .{other} files, only .stl and .3mf")),
            };
            std::fs::write(&output, &bytes).map_err(|e| format!("cannot write {}: {e}", output.display()))?;
            println!("{}: {} triangles, {} bytes", output.display(), mesh.triangles.len(), bytes.len());
            Ok(())
        }
    }
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
