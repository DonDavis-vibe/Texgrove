//! The heavy work, done in background jobs: loading a model, texturing it for a look or for
//! keeps, and the files that pass between jobs and the panel.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use meshgrove_plugin_api::MeshData;
use meshgrove_plugin_api::guest::{self, PluginError, library};
use serde::{Deserialize, Serialize};
use texgrove_core::export::{stl, threemf};
use texgrove_core::heightmap::Heightmap;
use texgrove_core::mask::{Dab, DabKind, Op};
use texgrove_core::pipeline::{self, Finish, Stage, Textured};
use texgrove_core::{Mesh, weld};

use crate::recipe::{self, On, Recipe};

/// Where previews are kept between a job and the panel.
pub const PREVIEW_DIR: &str = "/data/preview";
/// How many triangles a model is read with for a look at it.
const PREVIEW_SOURCE: u32 = 400_000;
/// How many triangles a textured look may have.
pub const PREVIEW_TRIANGLES: usize = 600_000;
/// How many triangles a model is read with for the file to keep (the most Meshgrove gives).
const EXPORT_SOURCE: u32 = 8_000_000;

/// A model to texture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Source {
    /// What `library` takes: a library id, or a path below `/data` for a file picked from
    /// outside the library.
    pub reference: String,
    /// The file's name, for people and for the new file's name.
    pub name: String,
    /// The objects of a 3MF, as Meshgrove lists them; empty for other files.
    pub parts: Vec<Part>,
    /// The tokens of the parts to leave as they are.
    pub skipped: Vec<String>,
    /// What the user painted on the model, stroke by stroke, in the coordinates of the model
    /// as shown.
    pub strokes: Vec<Stroke>,
}

/// What one press of the mouse did: a click on a face, or a line of brush dabs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Stroke {
    /// The layer it was painted for, by the layer's `uid`.
    #[serde(default)]
    pub layer: u32,
    pub ops: Vec<PaintOp>,
}

/// One mark of the user's on a model.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PaintOp {
    /// A click on a face: its whole smooth surface.
    Fill { point: [f32; 3], normal: [f32; 3], include: bool, erase: bool },
    /// A touch of the brush.
    Dab { centre: [f32; 3], normal: [f32; 3], radius: f32, hardness: f32, include: bool, erase: bool },
    /// Everything before is forgotten.
    Clear,
}

/// Faces differing by no more than this many degrees count as one surface for a click.
const FILL_DEGREES: f32 = 30.0;

/// One object of a 3MF.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Part {
    /// What `library.triangles` takes for this object alone, in its own coordinates.
    pub token: String,
    pub name: String,
    pub triangles: u64,
    /// Where it stands: the three axes and the shift, as 3MF writes it.
    pub transform: Vec<f32>,
}

impl Part {
    /// The three axes and the shift of where the object stands; none when there is nothing to do.
    fn frame(&self) -> Option<([[f32; 3]; 3], [f32; 3])> {
        match self.transform[..] {
            [xx, xy, xz, yx, yy, yz, zx, zy, zz, tx, ty, tz] => {
                Some(([[xx, xy, xz], [yx, yy, yz], [zx, zy, zz]], [tx, ty, tz]))
            }
            _ => None,
        }
    }

    /// A direction of the model as shown, in the object's own coordinates.
    pub fn dir_to_local(&self, d: [f32; 3]) -> [f32; 3] {
        use texgrove_core::vec::{cross, dot, normalize};
        let Some(([x, y, z], _)) = self.frame() else { return d };
        // The inverse of the matrix with columns x, y, z: its rows are the cross products of
        // the others, over the determinant.
        let det = dot(x, cross(y, z));
        if det.abs() < 1e-12 {
            return d;
        }
        normalize([dot(cross(y, z), d) / det, dot(cross(z, x), d) / det, dot(cross(x, y), d) / det])
    }

    /// A point of the model as shown, in the object's own coordinates.
    pub fn to_local(&self, p: [f32; 3]) -> [f32; 3] {
        use texgrove_core::vec::{cross, dot, sub};
        let Some(([x, y, z], shift)) = self.frame() else { return p };
        let det = dot(x, cross(y, z));
        if det.abs() < 1e-12 {
            return p;
        }
        let d = sub(p, shift);
        [dot(cross(y, z), d) / det, dot(cross(z, x), d) / det, dot(cross(x, y), d) / det]
    }

    /// The object's mesh moved to where it stands.
    fn placed(&self, mut mesh: Mesh) -> Mesh {
        if let [xx, xy, xz, yx, yy, yz, zx, zy, zz, tx, ty, tz] = self.transform[..] {
            for p in &mut mesh.positions {
                let [x, y, z] = *p;
                *p = [x * xx + y * yx + z * zx + tx, x * xy + y * yy + z * zy + ty, x * xz + y * yz + z * zz + tz];
            }
        }
        mesh
    }
}

impl Source {
    /// Whether the model lives in a library (and the result can go beside it).
    pub fn in_library(&self) -> bool {
        !self.reference.starts_with('/')
    }

    /// The library it lives in: the first part of its id.
    pub fn library(&self) -> Option<&str> {
        self.in_library().then(|| self.reference.split('/').next().unwrap_or_default())
    }

    /// Asks Meshgrove for the objects of a 3MF. Other files, and a 3MF Meshgrove cannot take
    /// apart, keep none.
    pub fn find_parts(&mut self) {
        if !self.name.to_ascii_lowercase().ends_with(".3mf") {
            return;
        }
        let Ok(facts) = library::describe(&self.reference) else { return };
        let facts: serde_json::Value = serde_json::from_str(&facts).unwrap_or_default();
        self.parts = facts
            .get("parts")
            .and_then(|parts| serde_json::from_value::<Vec<Part>>(parts.clone()).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|part| !part.token.is_empty())
            .collect();
    }

    /// The user's strokes for one layer for the kernel, in the coordinates of `part` (or the
    /// model's own).
    pub fn ops(&self, part: Option<&Part>, layer: u32) -> Vec<Op> {
        let point = |p: [f32; 3]| part.map_or(p, |part| part.to_local(p));
        let dir = |d: [f32; 3]| part.map_or(d, |part| part.dir_to_local(d));
        let kind = |include: bool| if include { DabKind::Include } else { DabKind::Exclude };
        self.strokes
            .iter()
            .filter(|stroke| stroke.layer == layer)
            .flat_map(|stroke| &stroke.ops)
            .map(|op| match *op {
                PaintOp::Clear => Op::Clear,
                PaintOp::Fill { point: p, normal, include, erase } => {
                    Op::Fill { point: point(p), normal: dir(normal), kind: kind(include), erase, degrees: FILL_DEGREES }
                }
                PaintOp::Dab { centre, normal, radius, hardness, include, erase } => Op::Dab(Dab {
                    centre: point(centre),
                    normal: dir(normal),
                    radius,
                    hardness,
                    kind: kind(include),
                    erase,
                }),
            })
            .collect()
    }

    /// The parts that get the texture.
    pub fn chosen(&self) -> impl Iterator<Item = &Part> {
        self.parts.iter().filter(|part| !self.skipped.contains(&part.token))
    }

    /// The name without its ending.
    pub fn stem(&self) -> &str {
        match self.name.rsplit_once('.') {
            Some((stem, _)) if !stem.is_empty() => stem,
            _ => &self.name,
        }
    }
}

/// What a preview job gets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewJob {
    pub source: Source,
    /// The textures, the lowest first.
    pub layers: Vec<Recipe>,
    pub nozzle: f32,
    /// The file names (below [`PREVIEW_DIR`]) to write the model as it is and as textured.
    pub plain: String,
    pub textured: String,
    /// The file names for which triangles of each get no texture.
    pub plain_marks: String,
    pub textured_marks: String,
}

/// What an export job gets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportJob {
    pub source: Source,
    /// The textures, the lowest first.
    pub layers: Vec<Recipe>,
    pub nozzle: f32,
    /// The file to write below `/out`.
    pub file: String,
}

impl ExportJob {
    /// The format of the file: the first layer carries it.
    fn format(&self) -> recipe::Format {
        self.layers.first().map_or(recipe::Format::ThreeMf, |layer| layer.format)
    }
}

/// What an export job answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exported {
    pub file: String,
    /// For a 3MF whose objects are swapped: each part's token and the new mesh below `/out`.
    #[serde(default)]
    pub replace: Vec<(String, String)>,
    pub triangles: usize,
    pub refined: usize,
    pub edge: f32,
    /// What the user should know about the file.
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl Exported {
    /// The file as it goes into a bundle: beside its model when that lives in a library, and
    /// for a project whose objects were textured one by one, as a copy with those swapped.
    pub fn bundle_file(&self, source: &Source, note: String) -> meshgrove_plugin_api::BundleFile {
        use meshgrove_plugin_api::{BundleFile, Derive3mf, PartMesh};
        let derive_3mf = (!self.replace.is_empty()).then(|| Derive3mf {
            source: source.reference.clone(),
            replace: self
                .replace
                .iter()
                .map(|(part, mesh)| PartMesh { part: part.clone(), mesh: mesh.clone() })
                .collect(),
        });
        BundleFile {
            path: self.file.clone(),
            note: Some(note),
            next_to: source.in_library().then(|| source.reference.clone()),
            derive_3mf,
            ..BundleFile::default()
        }
    }
}

/// The name a textured file gets: the model's, the texture's, the format's ending.
pub fn file_name(stem: &str, number: usize, layers: &[Recipe]) -> String {
    let texture = layers.first().map_or_else(String::new, |layer| crate::textures::name(&layer.texture));
    let extension = layers.first().map_or("3mf", |layer| layer.format.extension());
    let number = if number > 1 { format!(" {number}") } else { String::new() };
    format!("{stem}{number} – {texture}.{extension}").replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
}

/// What was done to a model, in words, for the note on its file.
pub fn note(source: &Source, layers: &[Recipe], triangles: usize) -> String {
    let strokes = |layer: &Recipe| source.strokes.iter().filter(|stroke| stroke.layer == layer.uid).count();
    let what = match layers {
        [only] => only.describe(strokes(only)),
        layers => layers
            .iter()
            .enumerate()
            .map(|(i, layer)| crate::tf!("Ebene {}: {}", "Layer {}: {}", i + 1, layer.describe(strokes(layer))))
            .collect::<Vec<_>>()
            .join("; "),
    };
    crate::tf!(
        "Texgrove {}: {}. Aus {}, {} Dreiecke.",
        "Texgrove {}: {}. Made from {}, {} triangles.",
        env!("CARGO_PKG_VERSION"),
        what,
        source.name,
        crate::lang::count(triangles),
    )
}

/// A warning of the kernel in words.
fn warning_text(warning: &pipeline::Warning) -> String {
    use crate::lang::number;
    match warning {
        pipeline::Warning::TooDeep { depth, smallest } => crate::tf!(
            "Die Textur geht {} mm tief, das Teil ist an der dünnsten Stelle nur {} mm: Wände können sich berühren.",
            "The texture goes {} mm deep and the part is only {} mm at its thinnest: walls may touch.",
            number(*depth, 2),
            number(*smallest, 1)
        ),
        pipeline::Warning::Flattened { triangles } => crate::tf!(
            "An {} Stellen war die Textur zu steil für das Netz und wurde dort flacher gemacht (feinere Qualität hilft).",
            "In {} places the texture was too steep for the mesh and was flattened there (a finer quality helps).",
            crate::lang::count(*triangles)
        ),
    }
}

/// A name for the cache file of what `parts` describe.
pub fn cache_key(parts: &impl Hash) -> String {
    let mut hasher = DefaultHasher::new();
    parts.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn failed(what: &str, error: impl std::fmt::Display) -> PluginError {
    PluginError::Failed(format!("{what}: {error}"))
}

/// Reads a model through Meshgrove and joins its corners.
pub fn load(source: &Source, budget: u32) -> Result<Mesh, PluginError> {
    let data: MeshData = library::triangles(&source.reference, budget).map_err(PluginError::Denied)?.into();
    let mesh = Mesh::from_flat(&data.positions, &data.indices).map_err(|e| failed(&source.name, e))?;
    if mesh.triangles.is_empty() {
        return Err(PluginError::Failed(crate::tf!("{} hat keine Dreiecke", "{} has no triangles", source.name)));
    }
    Ok(weld::weld_default(&mesh).mesh)
}

/// Lets the work go on unless the user stopped it, and tells Meshgrove how far it is.
fn progress(stage: Stage, done: f32) -> bool {
    let (from, to, what) = match stage {
        Stage::Refine => (0.05, 0.25, crate::lang::tr("Unterteilen", "Refining")),
        Stage::Move => (0.25, 0.35, crate::lang::tr("Textur auftragen", "Applying the texture")),
        Stage::Reduce => (0.35, 0.95, crate::lang::tr("Reduzieren", "Reducing")),
    };
    guest::host::progress(from + (to - from) * done.clamp(0.0, 1.0), what);
    !guest::host::cancelled()
}

/// The height maps of the layers.
fn maps(layers: &[Recipe]) -> Result<Vec<Heightmap>, PluginError> {
    if layers.is_empty() {
        return Err(PluginError::Invalid("no texture".into()));
    }
    layers.iter().map(|layer| layer.map().map_err(PluginError::Failed)).collect()
}

/// Puts the layers onto one mesh (a model, or one object of a project). Also says, for every
/// triangle of the mesh as it came, whether no layer may touch it.
fn texture(
    mesh: &Mesh,
    source: &Source,
    part: Option<&Part>,
    layers: &[Recipe],
    maps: &[Heightmap],
    finish: &Finish,
) -> Result<(Textured, Vec<bool>), PluginError> {
    let built: Vec<pipeline::Layer> = layers
        .iter()
        .zip(maps)
        .map(|(layer, map)| layer.layer(mesh, map, &On { ops: &source.ops(part, layer.uid), part }))
        .collect();
    let mut untouched = vec![true; mesh.triangles.len()];
    for layer in &built {
        for (out, active) in untouched.iter_mut().zip(layer.mask.active(mesh)) {
            *out &= !active;
        }
    }
    let done = pipeline::texture_layers(mesh, &built, finish, &mut progress).ok_or(PluginError::Cancelled)?;
    Ok((done, untouched))
}

/// A model as it is and textured coarsely, each with which of its triangles get no texture.
pub struct Look {
    pub plain: Mesh,
    pub plain_out: Vec<bool>,
    pub textured: Mesh,
    pub textured_out: Vec<bool>,
    pub warnings: Vec<String>,
}

/// Textures a model coarsely for a look at it: read with at most `read` triangles, refined to
/// about `budget`, nothing reduced.
pub fn look(source: &Source, layers: &[Recipe], nozzle: f32, read: u32, budget: usize) -> Result<Look, PluginError> {
    let maps = maps(layers)?;
    let mut look = Look {
        plain: Mesh::default(),
        plain_out: Vec::new(),
        textured: Mesh::default(),
        textured_out: Vec::new(),
        warnings: Vec::new(),
    };
    // A model is one mesh; a project is its objects, each textured in its own coordinates, as
    // the file to keep will have it, and then put where it stands.
    let whole = [None];
    let parts: Vec<Option<&Part>> = source.parts.iter().map(Some).collect();
    let pieces: &[Option<&Part>] = if parts.is_empty() { &whole } else { &parts };
    for piece in pieces {
        let (mesh, skipped) = match piece {
            None => (load(source, read)?, false),
            Some(part) => (load_part(source, part, read / pieces.len() as u32)?, source.skipped.contains(&part.token)),
        };
        let place = |mesh: Mesh| match piece {
            Some(part) => part.placed(mesh),
            None => mesh,
        };
        // A model with holes or fins still gets its texture, but the user should know.
        let report = texgrove_core::diagnostics::diagnose(&mesh);
        if !skipped && !report.closed() {
            let name = piece.map_or(source.name.as_str(), |part| part.name.as_str());
            look.warnings.push(crate::tf!(
                "{} ist nicht sauber geschlossen ({} offene Kanten, {} mit mehr als zwei Flächen). Die Textur wird trotzdem aufgetragen; der Slicer repariert so etwas meist selbst.",
                "{} is not cleanly closed ({} open edges, {} with more than two faces). The texture goes on all the same; a slicer usually repairs this by itself.",
                name,
                report.open_edges,
                report.non_manifold_edges
            ));
        }
        if skipped {
            look.plain_out.extend(std::iter::repeat_n(true, mesh.triangles.len()));
            look.textured_out.extend(std::iter::repeat_n(true, mesh.triangles.len()));
            look.plain.append(&place(mesh.clone()));
            look.textured.append(&place(mesh));
            continue;
        }
        let finish = recipe::preview_finish(layers, &maps, nozzle, budget / pieces.len());
        let (done, untouched) = texture(&mesh, source, *piece, layers, &maps, &finish)?;
        look.plain_out.extend(untouched);
        look.warnings.extend(done.warnings.iter().map(warning_text));
        look.plain.append(&place(mesh));
        look.textured_out.extend(&done.left_out);
        look.textured.append(&place(done.mesh));
    }
    Ok(look)
}

/// The preview: the model as it is and textured coarsely, each with a list of the triangles
/// that get no texture, written for the panel to show.
pub fn preview(job: &PreviewJob) -> Result<serde_json::Value, PluginError> {
    guest::host::progress(0.0, crate::lang::tr("Modell laden", "Loading the model"));
    let _ = std::fs::create_dir_all(PREVIEW_DIR);
    let look = look(&job.source, &job.layers, job.nozzle, PREVIEW_SOURCE, PREVIEW_TRIANGLES)?;
    write_mesh(&format!("{PREVIEW_DIR}/{}", job.plain), &look.plain)?;
    write_marks(&format!("{PREVIEW_DIR}/{}", job.plain_marks), &look.plain_out)?;
    write_mesh(&format!("{PREVIEW_DIR}/{}", job.textured), &look.textured)?;
    write_marks(&format!("{PREVIEW_DIR}/{}", job.textured_marks), &look.textured_out)?;
    prune(PREVIEW_DIR, 24);
    Ok(serde_json::json!({"triangles": look.textured.triangles.len(), "warnings": look.warnings}))
}

/// One object of a 3MF through Meshgrove, its corners joined.
fn load_part(source: &Source, part: &Part, budget: u32) -> Result<Mesh, PluginError> {
    let named =
        Source { reference: part.token.clone(), name: format!("{} ({})", source.name, part.name), ..Source::default() };
    load(&named, budget)
}

/// The file to keep, written below `/out`. A 3MF to be kept as 3MF only gets its textured
/// objects' meshes written; Meshgrove swaps them into a copy of the file.
pub fn export(job: &ExportJob) -> Result<Exported, PluginError> {
    guest::host::progress(0.0, crate::lang::tr("Modell laden", "Loading the model"));
    let maps = maps(&job.layers)?;
    if !job.source.parts.is_empty() {
        return export_parts(job, &maps);
    }
    let source = load(&job.source, EXPORT_SOURCE)?;
    let finish = recipe::finish(&job.layers, &maps, job.nozzle);
    let (textured, _) = texture(&source, &job.source, None, &job.layers, &maps, &finish)?;
    guest::host::progress(0.95, crate::lang::tr("Datei schreiben", "Writing the file"));
    write_file(job, &textured.mesh)?;
    Ok(Exported {
        file: job.file.clone(),
        replace: Vec::new(),
        triangles: textured.mesh.triangles.len(),
        refined: textured.refined_triangles,
        edge: textured.edge,
        warnings: textured.warnings.iter().map(warning_text).collect(),
    })
}

fn export_parts(job: &ExportJob, maps: &[Heightmap]) -> Result<Exported, PluginError> {
    let chosen: Vec<&Part> = job.source.chosen().collect();
    if chosen.is_empty() {
        return Err(PluginError::Invalid(crate::lang::tr("Kein Teil gewählt", "No part chosen").into()));
    }
    let as_3mf = job.format() == recipe::Format::ThreeMf;
    let parts: Vec<&Part> = if as_3mf { chosen.clone() } else { job.source.parts.iter().collect() };
    let budget = EXPORT_SOURCE / parts.len().max(1) as u32;
    let mut meshes = Vec::with_capacity(parts.len());
    for part in &parts {
        meshes.push(load_part(&job.source, part, budget)?);
    }
    // The triangles of the result are shared out by the textured surface.
    let areas: Vec<f64> = meshes.iter().map(Mesh::area).collect();
    let textured_area: f64 =
        (0..parts.len()).filter(|&i| chosen.contains(&parts[i])).map(|i| areas[i]).sum::<f64>().max(1e-9);
    let (mut total, mut refined, mut edge) = (0usize, 0usize, 0.0f32);
    let mut warnings = Vec::new();
    let mut replace = Vec::new();
    let mut whole = Mesh::default();
    let folder = format!("parts/{}", crate::work::cache_key(&job.file));
    for (i, part) in parts.iter().enumerate() {
        let mesh = std::mem::take(&mut meshes[i]);
        let mesh = if chosen.contains(part) {
            let mut finish = recipe::finish(&job.layers, maps, job.nozzle);
            let share = areas[i].max(0.0) / textured_area;
            finish.triangles = ((finish.triangles as f64 * share) as usize).max(20_000);
            let (done, _) = texture(&mesh, &job.source, Some(part), &job.layers, maps, &finish)?;
            refined += done.refined_triangles;
            warnings.extend(done.warnings.iter().map(warning_text));
            edge = edge.max(done.edge);
            done.mesh
        } else {
            mesh
        };
        total += mesh.triangles.len();
        if as_3mf {
            std::fs::create_dir_all(format!("/out/{folder}")).map_err(|e| failed(&job.file, e))?;
            let path = format!("{folder}/{i}.stl");
            std::fs::write(format!("/out/{path}"), stl::to_bytes(&mesh, &part.name))
                .map_err(|e| failed(&job.file, e))?;
            replace.push((part.token.clone(), path));
        } else {
            whole.append(&part.placed(mesh));
        }
    }
    guest::host::progress(0.95, crate::lang::tr("Datei schreiben", "Writing the file"));
    if !as_3mf {
        write_file(job, &whole)?;
    }
    Ok(Exported { file: job.file.clone(), replace, triangles: total, refined, edge, warnings })
}

/// Writes the mesh as the file the job names, in the format the recipe says.
fn write_file(job: &ExportJob, mesh: &Mesh) -> Result<(), PluginError> {
    let path = format!("/out/{}", job.file);
    let name = job.file.rsplit_once('.').map_or(job.file.as_str(), |(stem, _)| stem);
    let bytes = match job.format() {
        crate::recipe::Format::Stl => stl::to_bytes(mesh, name),
        crate::recipe::Format::ThreeMf => {
            let object = threemf::Object { name, mesh, transform: None };
            threemf::to_bytes(name, &[object]).map_err(|e| failed(&job.file, e))?
        }
    };
    std::fs::write(&path, bytes).map_err(|e| failed(&job.file, e))
}

/// A mesh as raw little-endian numbers: vertex and triangle count, then positions and corners.
fn write_mesh(path: &str, mesh: &Mesh) -> Result<(), PluginError> {
    let mut bytes = Vec::with_capacity(8 + mesh.positions.len() * 12 + mesh.triangles.len() * 12);
    bytes.extend((mesh.positions.len() as u32).to_le_bytes());
    bytes.extend((mesh.triangles.len() as u32).to_le_bytes());
    for c in mesh.positions.as_flattened() {
        bytes.extend(c.to_le_bytes());
    }
    for v in mesh.triangles.as_flattened() {
        bytes.extend(v.to_le_bytes());
    }
    std::fs::write(path, bytes).map_err(|e| failed(path, e))
}

/// Which triangles get no texture, as the 3D view takes an overlay: a count, then a class per
/// triangle (1 for left out, 0 for textured).
fn write_marks(path: &str, left_out: &[bool]) -> Result<(), PluginError> {
    let mut bytes = Vec::with_capacity(4 + left_out.len() * 4);
    bytes.extend((left_out.len() as u32).to_le_bytes());
    for out in left_out {
        bytes.extend(u32::from(*out).to_le_bytes());
    }
    std::fs::write(path, bytes).map_err(|e| failed(path, e))
}

/// The classes [`write_marks`] wrote, as an overlay: no positions, a class per triangle.
pub fn read_marks(path: &str) -> Option<MeshData> {
    let bytes = std::fs::read(path).ok()?;
    let count = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?) as usize;
    let (classes, rest) = bytes[4..].as_chunks::<4>();
    (classes.len() == count && rest.is_empty())
        .then(|| MeshData { positions: Vec::new(), indices: classes.iter().map(|b| u32::from_le_bytes(*b)).collect() })
}

/// A mesh [`write_mesh`] wrote, as the 3D view wants it.
pub fn read_mesh(path: &str) -> Option<MeshData> {
    let bytes = std::fs::read(path).ok()?;
    let count = |at: usize| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize);
    let (vertices, triangles) = (count(0)?, count(4)?);
    let corners = 8 + vertices * 12;
    if bytes.len() != corners + triangles * 12 {
        return None;
    }
    let (floats, _) = bytes[8..corners].as_chunks::<4>();
    let (ints, _) = bytes[corners..].as_chunks::<4>();
    Some(MeshData {
        positions: floats.iter().map(|b| f32::from_le_bytes(*b)).collect(),
        indices: ints.iter().map(|b| u32::from_le_bytes(*b)).collect(),
    })
}

/// Whether a preview file is there.
pub fn exists(name: &str) -> bool {
    std::fs::metadata(format!("{PREVIEW_DIR}/{name}")).is_ok()
}

/// Keeps the `keep` newest files of a folder.
fn prune(dir: &str, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<_> =
        entries.filter_map(Result::ok).filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
    if files.len() <= keep {
        return;
    }
    files.sort();
    for (_, path) in &files[..files.len() - keep] {
        let _ = std::fs::remove_file(path);
    }
}
