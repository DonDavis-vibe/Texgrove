//! The heavy work, done in background jobs: loading a model, texturing it for a look or for
//! keeps, and the files that pass between jobs and the panel.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use meshgrove_plugin_api::MeshData;
use meshgrove_plugin_api::guest::{self, PluginError, library};
use serde::{Deserialize, Serialize};
use texgrove_core::export::{stl, threemf};
use texgrove_core::pipeline::{self, Stage};
use texgrove_core::{Mesh, weld};

use crate::recipe::Recipe;

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
}

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
    pub recipe: Recipe,
    pub nozzle: f32,
    /// The file names (below [`PREVIEW_DIR`]) to write the model as it is and as textured.
    pub plain: String,
    pub textured: String,
}

/// What an export job gets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportJob {
    pub source: Source,
    pub recipe: Recipe,
    pub nozzle: f32,
    /// The file to write below `/out`.
    pub file: String,
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
fn load(source: &Source, budget: u32) -> Result<Mesh, PluginError> {
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

/// The preview: the model as it is and textured coarsely, written for the panel to show.
pub fn preview(job: &PreviewJob) -> Result<serde_json::Value, PluginError> {
    guest::host::progress(0.0, crate::lang::tr("Modell laden", "Loading the model"));
    let source = load(&job.source, PREVIEW_SOURCE)?;
    let _ = std::fs::create_dir_all(PREVIEW_DIR);
    write_mesh(&format!("{PREVIEW_DIR}/{}", job.plain), &source)?;
    let map = job.recipe.map().map_err(PluginError::Failed)?;
    let textured = if job.source.parts.is_empty() {
        let settings = job.recipe.preview_settings(&source, job.nozzle, PREVIEW_TRIANGLES, &map);
        pipeline::texture_with(&source, &map, &settings, &mut progress).ok_or(PluginError::Cancelled)?.mesh
    } else {
        // Each object on its own, in its own coordinates as the file to keep will have it, then
        // put where it stands.
        let mut whole = Mesh::default();
        let budget = PREVIEW_TRIANGLES / job.source.parts.len().max(1);
        for part in &job.source.parts {
            let mesh = load_part(&job.source, part, PREVIEW_SOURCE / job.source.parts.len().max(1) as u32)?;
            let mesh = if job.source.skipped.contains(&part.token) {
                mesh
            } else {
                let settings = job.recipe.preview_settings(&mesh, job.nozzle, budget, &map);
                pipeline::texture_with(&mesh, &map, &settings, &mut progress).ok_or(PluginError::Cancelled)?.mesh
            };
            whole.append(&part.placed(mesh));
        }
        whole
    };
    write_mesh(&format!("{PREVIEW_DIR}/{}", job.textured), &textured)?;
    prune(PREVIEW_DIR, 12);
    Ok(serde_json::json!({"triangles": textured.triangles.len()}))
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
    let map = job.recipe.map().map_err(PluginError::Failed)?;
    if !job.source.parts.is_empty() {
        return export_parts(job, &map);
    }
    let source = load(&job.source, EXPORT_SOURCE)?;
    let settings = job.recipe.settings(&source, job.nozzle, &map);
    let textured = pipeline::texture_with(&source, &map, &settings, &mut progress).ok_or(PluginError::Cancelled)?;
    guest::host::progress(0.95, crate::lang::tr("Datei schreiben", "Writing the file"));
    write_file(job, &textured.mesh)?;
    Ok(Exported {
        file: job.file.clone(),
        replace: Vec::new(),
        triangles: textured.mesh.triangles.len(),
        refined: textured.refined_triangles,
        edge: textured.edge,
    })
}

fn export_parts(job: &ExportJob, map: &texgrove_core::heightmap::Heightmap) -> Result<Exported, PluginError> {
    let chosen: Vec<&Part> = job.source.chosen().collect();
    if chosen.is_empty() {
        return Err(PluginError::Invalid(crate::lang::tr("Kein Teil gewählt", "No part chosen").into()));
    }
    let as_3mf = job.recipe.format == crate::recipe::Format::ThreeMf;
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
    let mut replace = Vec::new();
    let mut whole = Mesh::default();
    let folder = format!("parts/{}", crate::work::cache_key(&job.file));
    for (i, part) in parts.iter().enumerate() {
        let mesh = std::mem::take(&mut meshes[i]);
        let mesh = if chosen.contains(part) {
            let mut settings = job.recipe.settings(&mesh, job.nozzle, map);
            let share = areas[i].max(0.0) / textured_area;
            settings.finish.triangles = ((settings.finish.triangles as f64 * share) as usize).max(20_000);
            let done = pipeline::texture_with(&mesh, map, &settings, &mut progress).ok_or(PluginError::Cancelled)?;
            refined += done.refined_triangles;
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
    Ok(Exported { file: job.file.clone(), replace, triangles: total, refined, edge })
}

/// Writes the mesh as the file the job names, in the format the recipe says.
fn write_file(job: &ExportJob, mesh: &Mesh) -> Result<(), PluginError> {
    let path = format!("/out/{}", job.file);
    let name = job.file.rsplit_once('.').map_or(job.file.as_str(), |(stem, _)| stem);
    let bytes = match job.recipe.format {
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
