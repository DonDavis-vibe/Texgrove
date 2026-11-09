//! Texgrove in Meshgrove: textures as real geometry on library models.
//!
//! - Right-click models in the gallery → "Apply texture …" opens the panel with them (their
//!   library ids; Texgrove may read exactly those files).
//! - The panel shows a coarse preview, made in a background job and kept in `/data/preview`.
//! - "Apply texture" makes the files one after another in background jobs and puts each beside
//!   its model (`next_to`), tagged and with the settings in its note: one step in the history
//!   per library.

mod catalog;
mod lang;
mod panel;
mod recipe;
mod textures;
mod thumbs;
mod work;

use meshgrove_plugin_api::guest::{self, Guest, PluginError, ToolOutput};
use meshgrove_plugin_api::{API_VERSION, FileType, Manifest, PanelDef, Permission, Receive, ToolDef};

struct Texgrove;

/// The model files Meshgrove can read and Texgrove takes.
const PATTERNS: [&str; 8] = ["*.stl", "*.3mf", "*.obj", "*.ply", "*.step", "*.stp", "*.glb", "*.gltf"];

impl Guest for Texgrove {
    type Panel = panel::TexturePanel;

    fn manifest() -> String {
        lang::set(&guest::info().language);
        let file_types = PATTERNS
            .iter()
            .map(|pattern| FileType {
                pattern: (*pattern).into(),
                description: lang::tr("Ein Modell zum Texturieren", "A model to texture").into(),
                label: Some(lang::tr("Textur auftragen …", "Apply texture …").into()),
                receive: Receive::Ids,
                panel: Some("texture".into()),
            })
            .collect();
        guest::json(&Manifest {
            id: "texgrove".into(),
            name: "Texgrove".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            api: API_VERSION.into(),
            description: lang::tr(
                "Texturen als echte Geometrie: Rändel, Waben, Geflecht und mehr auf deine Modelle. Inspiriert von BumpMesh (CNC Kitchen).",
                "Textures as real geometry: knurls, hexagons, weaves and more on your models. Inspired by BumpMesh (CNC Kitchen).",
            )
            .into(),
            authors: vec!["Davis (German Redneck Engineering)".into()],
            license: Some("MIT OR Apache-2.0".into()),
            homepage: Some("https://redneck-design.de/".into()),
            permissions: vec![Permission::Bundles, Permission::LibraryRead],
            panels: vec![PanelDef { id: "texture".into(), title: "Texgrove".into() }],
            file_types,
            memory_mb: Some(3584),
        })
    }

    fn list_tools() -> String {
        guest::json(&Vec::<ToolDef>::new())
    }

    fn call_tool(name: String, _args: String) -> Result<ToolOutput, PluginError> {
        Err(PluginError::Invalid(format!("no tool {name:?}")))
    }

    fn run_job(kind: String, params: String) -> Result<String, PluginError> {
        lang::set(&guest::info().language);
        match kind.as_str() {
            "preview" => Ok(guest::json(&work::preview(&guest::parse(&params)?)?)),
            "export" => Ok(guest::json(&work::export(&guest::parse(&params)?)?)),
            _ => Err(PluginError::Invalid(format!("no job {kind:?}"))),
        }
    }

    fn open_file(_name: String, _bytes: Vec<u8>) -> Result<String, PluginError> {
        Err(PluginError::Invalid("Texgrove opens models by their library ids".into()))
    }
}

meshgrove_plugin_api::export_plugin!(Texgrove);
