//! Tools for an AI: what textures there are, what a model is like, a picture of a model with a
//! texture on it, and making the textured file (in a background job; it waits in the review).

use meshgrove_plugin_api::guest::{self, Image, PluginError, ToolOutput, library};
use meshgrove_plugin_api::{Bundle, BundleItem, BundleJob, BundleList, ListKind, ToolDef};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use texgrove_core::bvh::Bvh;
use texgrove_core::diagnostics::diagnose;
use texgrove_core::raster::{self, Camera, Picture};
use texgrove_core::select;
use texgrove_core::topology::Topology;

use crate::lang::tr;
use crate::recipe::{Format, MappingChoice, Place, QualityChoice, Recipe, ReliefChoice};
use crate::textures::Group;
use crate::work::{self, ExportJob, Source};

/// How many triangles a model is read with for a tool's answer within its minute.
const TOOL_SOURCE: u32 = 150_000;
/// How many triangles a tool's preview may have.
const TOOL_PREVIEW: usize = 250_000;

/// What a tool takes to say how a texture goes onto a model. Everything but `id` may be left out.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Args {
    id: String,
    texture: Option<String>,
    mapping: Option<String>,
    tile_mm: Option<f32>,
    depth_mm: Option<f32>,
    direction: Option<String>,
    rotation: Option<f32>,
    smoothing: Option<f32>,
    invert: Option<bool>,
    #[serde(rename = "where")]
    place: Option<String>,
    bottom_angle: Option<f32>,
    top_angle: Option<f32>,
    quality: Option<String>,
    format: Option<String>,
    /// For a project (3MF) with several objects: the names of those to texture.
    parts: Vec<String>,
    /// A print list to put the new file onto as well.
    print_list: Option<String>,
}

fn invalid(what: impl Into<String>) -> PluginError {
    PluginError::Invalid(what.into())
}

/// All textures: id, name, group.
fn all_textures() -> Vec<(String, String, Group)> {
    Group::KINDS
        .iter()
        .flat_map(|group| group.textures().into_iter().map(move |(id, name)| (id, name, *group)))
        .collect()
}

impl Args {
    /// The recipe the arguments ask for, checked.
    fn recipe(&self) -> Result<Recipe, PluginError> {
        let mut recipe = Recipe::default();
        if let Some(texture) = &self.texture {
            if !all_textures().iter().any(|(id, ..)| id == texture) {
                return Err(invalid(format!("no texture \"{texture}\"; the textures tool lists them")));
            }
            recipe.texture = texture.clone();
        }
        let ids = |all: &[&str]| all.join(", ");
        if let Some(mapping) = &self.mapping {
            recipe.mapping =
                MappingChoice::from_id(mapping).filter(|m| *m != MappingChoice::Stamp).ok_or_else(|| {
                    invalid(format!("mapping is one of: {}", ids(&MappingChoice::ALL.map(MappingChoice::id)[..8])))
                })?;
        }
        if let Some(direction) = &self.direction {
            recipe.relief = ReliefChoice::from_id(direction).ok_or_else(|| {
                invalid(format!("direction is one of: {}", ids(&ReliefChoice::ALL.map(ReliefChoice::id))))
            })?;
        }
        if let Some(place) = &self.place {
            recipe.place = Place::from_id(place)
                .ok_or_else(|| invalid(format!("where is one of: {}", ids(&Place::ALL.map(Place::id)))))?;
        }
        if let Some(quality) = &self.quality {
            recipe.quality = QualityChoice::from_id(quality).ok_or_else(|| {
                invalid(format!("quality is one of: {}", ids(&QualityChoice::ALL.map(QualityChoice::id))))
            })?;
        }
        match self.format.as_deref() {
            None => {}
            Some("3mf") => recipe.format = Format::ThreeMf,
            Some("stl") => recipe.format = Format::Stl,
            Some(_) => return Err(invalid("format is 3mf or stl")),
        }
        let within = |name: &str, value: Option<f32>, low: f32, high: f32| match value {
            Some(v) if !v.is_finite() || v < low || v > high => {
                Err(invalid(format!("{name} must be between {low} and {high}")))
            }
            other => Ok(other),
        };
        if let Some(v) = within("tile_mm", self.tile_mm, 0.2, 200.0)? {
            recipe.tile = v;
        }
        if let Some(v) = within("depth_mm", self.depth_mm, 0.0, 20.0)? {
            recipe.depth = v;
        }
        if let Some(v) = within("rotation", self.rotation, -360.0, 360.0)? {
            recipe.rotation = v;
        }
        if let Some(v) = within("smoothing", self.smoothing, 0.0, 25.0)? {
            recipe.smooth = v;
        }
        if let Some(v) = within("bottom_angle", self.bottom_angle, 0.0, 90.0)? {
            recipe.bottom = v;
        }
        if let Some(v) = within("top_angle", self.top_angle, 0.0, 90.0)? {
            recipe.top = v;
        }
        recipe.invert = self.invert.unwrap_or(false);
        Ok(recipe)
    }

    /// The model the arguments name, with a project's objects and which of them to leave.
    fn source(&self) -> Result<Source, PluginError> {
        if self.id.trim().is_empty() {
            return Err(invalid("id: the model's file id (from search_models) is missing"));
        }
        // Asking for its facts says at once whether the model may be read at all.
        library::describe(&self.id).map_err(PluginError::Denied)?;
        let name = self.id.rsplit('/').next().unwrap_or(&self.id).to_owned();
        let mut source = Source { reference: self.id.clone(), name, ..Source::default() };
        source.find_parts();
        if !self.parts.is_empty() {
            let wanted: Vec<String> = self.parts.iter().map(|p| p.to_lowercase()).collect();
            let names: Vec<String> = source.parts.iter().map(|p| p.name.clone()).collect();
            if let Some(unknown) = wanted.iter().find(|w| !names.iter().any(|n| n.to_lowercase() == **w)) {
                return Err(invalid(format!("the model has no part \"{unknown}\"; its parts: {}", names.join(", "))));
            }
            source.skipped = source
                .parts
                .iter()
                .filter(|p| !wanted.contains(&p.name.to_lowercase()))
                .map(|p| p.token.clone())
                .collect();
        }
        Ok(source)
    }
}

fn nozzle() -> f32 {
    guest::info().printers.first().and_then(|p| p.nozzle).filter(|n| *n > 0.0).unwrap_or(0.4)
}

/// The arguments every texturing tool takes, as a JSON schema.
fn recipe_schema(with_file: bool) -> Value {
    let mut properties = json!({
        "id": {"type": "string", "description": "The model's file id, from search_models."},
        "texture": {"type": "string", "description": "A texture id from the textures tool. Default: knurl."},
        "mapping": {"type": "string", "enum": ["triplanar", "cubic", "cylindrical", "outline", "spherical", "xy", "xz", "yz"],
            "description": "How the texture is laid on: triplanar (any shape, default), cubic (boxes), cylindrical or outline (round the vertical axis; outline follows the part's outline, good for box-like parts), spherical, or flat from above (xy), the front (xz), the side (yz)."},
        "tile_mm": {"type": "number", "description": "Width of one tile of the texture in mm (0.2 to 200). Default 5."},
        "depth_mm": {"type": "number", "description": "From the texture's lowest to its highest point in mm (0 to 20). Default 0.6; below about 0.3 an FDM printer hardly shows it."},
        "direction": {"type": "string", "enum": ["both", "raise", "sink"], "description": "both: bright parts out, dark parts in (default). raise: only outwards. sink: only inwards, the part keeps its outer size."},
        "rotation": {"type": "number", "description": "Turns the texture, in degrees."},
        "smoothing": {"type": "number", "description": "Blurs the texture, in percent of a tile (0 to 25); for grainy pictures."},
        "invert": {"type": "boolean", "description": "Swaps high and low."},
        "where": {"type": "string", "enum": ["all", "outside", "inside"], "description": "all (default); outside: not in bores, cups and other hollows, so fits keep their size; inside: only there."},
        "bottom_angle": {"type": "number", "description": "Faces looking down within this many degrees stay smooth, so the part lies flat on the bed. Default 5; 0 textures the bottom too."},
        "top_angle": {"type": "number", "description": "The same for faces looking up. Default 0."},
        "parts": {"type": "array", "items": {"type": "string"}, "description": "For a 3MF project with several objects: the names of the objects to texture (from inspect). Default: all."}
    });
    if with_file {
        properties["quality"] = json!({"type": "string", "enum": ["draft", "normal", "fine"], "description": "How fine the mesh is made. Default normal."});
        properties["format"] = json!({"type": "string", "enum": ["3mf", "stl"], "description": "Default 3mf. A 3MF project keeps its other objects, plates and settings."});
        properties["print_list"] = json!({"type": "string", "description": "Also put the new file onto the print list of this name; a list that does not exist yet is made. Only when the user asks for it."});
    }
    json!({"type": "object", "properties": properties, "required": ["id"]})
}

pub fn list() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "textures".into(),
            description: "Lists the textures Texgrove can put onto a model as real geometry (knurls, hexagons, wood, leather, …), with a picture of them. Use it to choose a texture id.".into(),
            input_schema: json!({"type": "object", "properties": {
                "query": {"type": "string", "description": "Only textures whose name or id contains this."}
            }}),
            bundles: false,
        },
        ToolDef {
            name: "inspect".into(),
            description: "Says what a model is like for texturing: its size, whether it is closed, its parts, how much of it is inside (bores, hollows), and sensible tile sizes and depths.".into(),
            input_schema: json!({"type": "object", "properties": {
                "id": {"type": "string", "description": "The model's file id, from search_models."}
            }, "required": ["id"]}),
            bundles: false,
        },
        ToolDef {
            name: "preview".into(),
            description: "Shows a picture of a model before and after a texture is put on, coarsely, without making a file. What gets no texture is orange in the right picture. Use it to check a choice before apply.".into(),
            input_schema: recipe_schema(false),
            bundles: false,
        },
        ToolDef {
            name: "apply".into(),
            description: "Puts a texture onto a model as real geometry and saves the result as a new file beside the model, tagged, with the settings in its note. The original is not changed. The file is made in the background and then waits in Meshgrove's review for the user.".into(),
            input_schema: recipe_schema(true),
            bundles: true,
        },
    ]
}

pub fn call(name: &str, args: &str) -> Result<ToolOutput, PluginError> {
    match name {
        "textures" => textures(args),
        "inspect" => inspect(&guest::parse(args)?),
        "preview" => preview(&guest::parse(args)?),
        "apply" => apply(&guest::parse(args)?),
        _ => Err(invalid(format!("no tool {name:?}"))),
    }
}

fn textures(args: &str) -> Result<ToolOutput, PluginError> {
    let query = serde_json::from_str::<Value>(args).ok().and_then(|v| v["query"].as_str().map(str::to_lowercase));
    let found: Vec<(String, String, Group)> = all_textures()
        .into_iter()
        .filter(|(id, name, _)| {
            query.as_ref().is_none_or(|q| id.to_lowercase().contains(q) || name.to_lowercase().contains(q))
        })
        .collect();
    let list: Vec<Value> = found
        .iter()
        .enumerate()
        .map(|(i, (id, name, group))| {
            let mut entry = json!({"number": i + 1, "id": id, "name": name, "group": group.label()});
            if let Some(credit) = crate::textures::credit(id) {
                entry["credit"] = credit.into();
            }
            entry
        })
        .collect();
    let sheet = crate::thumbs::sheet(&found.iter().map(|(id, ..)| id.as_str()).collect::<Vec<_>>());
    let result = json!({
        "textures": list,
        "picture": "The picture shows them in this order, numbered, bright is high.",
    });
    Ok(ToolOutput {
        result: guest::json(&result),
        images: sheet.map(|png| Image { mime: "image/png".into(), data: png }).into_iter().collect(),
        bundle: None,
    })
}

fn inspect(args: &Args) -> Result<ToolOutput, PluginError> {
    let source = args.source()?;
    let mesh = work::load(&source, TOOL_SOURCE)?;
    let report = diagnose(&mesh);
    // A big model comes simplified, which tears holes of its own: then nothing is said about
    // holes, and the triangle count is the file's.
    let simplified = work::simplified(&source, None, &mesh);
    let triangles = work::real_triangles(&source, None).unwrap_or(report.triangles);
    let size = report.bounds.size();
    let inside = select::inside(&mesh, &Topology::build(&mesh), &Bvh::build(&mesh), 30.0);
    let (mut inside_area, mut area) = (0.0f64, 0.0f64);
    for (t, inside) in inside.iter().enumerate() {
        let a = texgrove_core::vec::length64(mesh.area_vector(t)) / 2.0;
        area += a;
        if *inside {
            inside_area += a;
        }
    }
    let smallest = size[0].min(size[1]).min(size[2]);
    let round = |v: f32| (v * 10.0).round() / 10.0;
    let result = json!({
        "name": source.name,
        // Rounded as f64: a rounded f32 in a list is written out as 179.60000610351562.
        "size_mm": size.map(|v| (f64::from(v) * 10.0).round() / 10.0),
        "triangles": triangles,
        "closed": (!simplified).then(|| report.closed()),
        "pieces": report.shells,
        "open_edges": (!simplified).then_some(report.open_edges),
        "parts": source.parts.iter().map(|p| json!({"name": p.name, "triangles": p.triangles})).collect::<Vec<_>>(),
        "inside_share": if area > 0.0 { ((inside_area / area) * 100.0).round() / 100.0 } else { 0.0 },
        "hints": {
            "inside": "With where=outside, bores and other hollows keep their size; inside_share says how much of the surface that is.",
            "tile_mm": format!("{} to {} for a pattern that reads well on this size", round((smallest / 12.0).clamp(1.0, 8.0)), round((smallest / 4.0).clamp(2.0, 25.0))),
            "depth_mm": format!("0.3 to 0.8 shows well on an FDM printer; at most about {} here (a tenth of the smallest side)", round(smallest / 10.0)),
        },
    });
    Ok(guest::tool_output(&result, None))
}

fn preview(args: &Args) -> Result<ToolOutput, PluginError> {
    let source = args.source()?;
    let layers = [args.recipe()?];
    let look = work::look(&source, &layers, nozzle(), TOOL_SOURCE, TOOL_PREVIEW)?;
    let camera = Camera::default();
    let pictures = [
        raster::render(&look.plain, 384, &camera, None),
        raster::render(&look.textured, 384, &camera, Some(&look.textured_out)),
    ];
    // By area: what is left out is not refined, so it is few triangles however large it is.
    let (mut without, mut area) = (0.0f64, 0.0f64);
    for (t, out) in look.textured_out.iter().enumerate() {
        let a = texgrove_core::vec::length64(look.textured.area_vector(t)) / 2.0;
        area += a;
        if *out {
            without += a;
        }
    }
    let result = json!({
        "picture": "Left: the model as it is. Right: with the texture; orange gets none.",
        "settings": layers[0].describe(0),
        "warnings": look.warnings,
        "surface_without_texture": if area > 0.0 { ((without / area) * 100.0).round() / 100.0 } else { 0.0 },
    });
    Ok(ToolOutput {
        result: guest::json(&result),
        images: vec![Image { mime: "image/png".into(), data: Picture::beside(&pictures).to_png() }],
        bundle: None,
    })
}

/// What the background job of `apply` gets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyJob {
    pub export: ExportJob,
    pub title: String,
    /// The print list the file goes onto as well, if any.
    #[serde(default)]
    pub print_list: Option<String>,
}

fn apply(args: &Args) -> Result<ToolOutput, PluginError> {
    let source = args.source()?;
    let layers = vec![args.recipe()?];
    // Read once here, so a wrong texture or picture fails now and not in the background.
    layers[0].map().map_err(PluginError::Failed)?;
    let file = work::file_name(source.stem(), 1, &layers);
    let title = format!("{} – {}", source.stem(), crate::textures::name(&layers[0].texture));
    let job = ApplyJob {
        export: ExportJob { source: source.clone(), layers, nozzle: nozzle(), file: file.clone() },
        title: title.clone(),
        print_list: args.print_list.as_deref().map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned),
    };
    let bundle = Bundle {
        title,
        next_to: Some(source.reference.clone()),
        tags: vec![tr("Texturiert", "Textured").into()],
        job: Some(BundleJob { kind: "apply".into(), params: serde_json::to_value(&job).unwrap_or_default() }),
        ..Bundle::default()
    };
    let result = json!({
        "file": file,
        "beside": source.reference,
        "note": "The file is being made in the background (seconds to a few minutes) and will then wait in Meshgrove's review for the user to accept.",
    });
    Ok(guest::tool_output(&result, Some(&bundle)))
}

/// The background job of `apply`: makes the file and says how it goes into the library.
pub fn run_apply(job: &ApplyJob) -> Result<Value, PluginError> {
    let made = work::export(&job.export)?;
    let source = &job.export.source;
    let file = made.bundle_file(source, work::note(source, &job.export.layers, made.triangles));
    let lists = job
        .print_list
        .iter()
        .map(|name| BundleList {
            name: name.clone(),
            kind: ListKind::Print,
            items: vec![BundleItem { path: file.path.clone(), quantity: 1 }],
        })
        .collect();
    let bundle = Bundle {
        title: job.title.clone(),
        files: vec![file],
        tags: vec![tr("Texturiert", "Textured").into()],
        lists,
        ..Bundle::default()
    };
    Ok(json!({"bundle": bundle, "file": made.file, "triangles": made.triangles, "warnings": made.warnings}))
}
