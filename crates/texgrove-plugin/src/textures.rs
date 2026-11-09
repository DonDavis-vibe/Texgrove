//! The user's own textures: pictures picked with the file dialog, kept as greyscale PNGs in the
//! plugin's own folder, so jobs read them too and they are there after a restart.

use serde::{Deserialize, Serialize};
use texgrove_core::heightmap::Heightmap;
use texgrove_core::pattern::Pattern;
use texgrove_core::picture;

use crate::catalog::{Category, SHIPPED, Shipped};
use crate::lang::tr;
use crate::recipe::pattern_label;

const DIR: &str = "/data/textures";
const INDEX: &str = "/data/textures/index.json";
/// A texture id of the user's own starts with this.
pub const OWN: &str = "own:";

/// One of the user's textures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Own {
    /// What follows [`OWN`] in a texture id, and the file's name.
    pub id: String,
    /// The picture's file name without its ending.
    pub name: String,
}

/// The user's textures, oldest first.
pub fn list() -> Vec<Own> {
    std::fs::read(INDEX).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

fn save(list: &[Own]) -> Result<(), String> {
    std::fs::create_dir_all(DIR).map_err(|e| e.to_string())?;
    std::fs::write(INDEX, serde_json::to_vec_pretty(list).unwrap_or_default()).map_err(|e| e.to_string())
}

/// Reads a picture the user picked (a path below `/data` or `/tmp`) and keeps it; its texture id.
pub fn add(path: &str, name: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{name}: {e}"))?;
    let map = picture::read(&bytes).map_err(|e| format!("{name}: {e}"))?.normalized();
    let png = picture::to_png(&map);
    let id = crate::work::cache_key(&png);
    std::fs::create_dir_all(DIR).map_err(|e| e.to_string())?;
    std::fs::write(format!("{DIR}/{id}.png"), &png).map_err(|e| e.to_string())?;
    let mut list = list();
    if !list.iter().any(|own| own.id == id) {
        let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        list.push(Own { id: id.clone(), name: stem.chars().take(40).collect() });
        save(&list)?;
    }
    // The picked copy is not needed any more.
    if path.starts_with("/data/picked/") {
        let _ = std::fs::remove_file(path);
    }
    Ok(format!("{OWN}{id}"))
}

/// Forgets one of the user's textures.
pub fn remove(texture: &str) -> Result<(), String> {
    let Some(id) = texture.strip_prefix(OWN) else { return Ok(()) };
    let mut list = list();
    list.retain(|own| own.id != id);
    let _ = std::fs::remove_file(format!("{DIR}/{id}.png"));
    save(&list)
}

/// The height map behind a texture id: a built-in pattern, or one of the user's pictures.
pub fn map(texture: &str) -> Result<Heightmap, String> {
    if let Some(id) = texture.strip_prefix(OWN) {
        if !id.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(tr("Unbekannte Textur", "Unknown texture").into());
        }
        let bytes = std::fs::read(format!("{DIR}/{id}.png"))
            .map_err(|_| tr("Die eigene Textur ist nicht mehr da", "The texture of your own is gone").to_owned())?;
        return picture::read(&bytes).map_err(|e| e.to_string());
    }
    if let Some(shipped) = shipped(texture) {
        return picture::read(shipped.png).map_err(|e| e.to_string());
    }
    Ok(Pattern::from_id(texture).unwrap_or(Pattern::Knurl).render(1024))
}

/// A texture that ships with Texgrove, by its id.
pub fn shipped(texture: &str) -> Option<&'static Shipped> {
    SHIPPED.iter().find(|s| s.id == texture)
}

/// Where a shipped texture comes from and under which licence, for the panel and the note.
pub fn credit(texture: &str) -> Option<&'static str> {
    shipped(texture).map(|s| s.credit)
}

/// The groups of the gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Patterns,
    Nature,
    Hero,
    Own,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Patterns, Group::Nature, Group::Hero, Group::Own];

    pub fn id(self) -> &'static str {
        match self {
            Group::Patterns => "patterns",
            Group::Nature => "nature",
            Group::Hero => "hero",
            Group::Own => "own",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Group::Patterns => tr("Muster", "Patterns"),
            Group::Nature => tr("Natur & Material", "Nature & material"),
            Group::Hero => "Hero Patterns",
            Group::Own => tr("Eigene", "Your own"),
        }
    }

    pub fn from_id(id: &str) -> Group {
        Group::ALL.into_iter().find(|g| g.id() == id).unwrap_or(Group::Patterns)
    }

    /// The group a texture belongs to.
    pub fn of(texture: &str) -> Group {
        match shipped(texture).map(|s| s.category) {
            Some(Category::Nature) => Group::Nature,
            Some(Category::Hero) => Group::Hero,
            None if texture.starts_with(OWN) => Group::Own,
            None => Group::Patterns,
        }
    }

    /// The textures of the group: id and name.
    pub fn textures(self) -> Vec<(String, String)> {
        let shipped = |category: Category| {
            SHIPPED
                .iter()
                .filter(|s| s.category == category)
                .map(|s| (s.id.to_owned(), tr(s.de, s.en).to_owned()))
                .collect()
        };
        match self {
            Group::Patterns => Pattern::ALL.iter().map(|p| (p.id().to_owned(), pattern_label(*p).to_owned())).collect(),
            Group::Nature => shipped(Category::Nature),
            Group::Hero => shipped(Category::Hero),
            Group::Own => list().into_iter().map(|own| (format!("{OWN}{}", own.id), own.name)).collect(),
        }
    }
}

/// The name of a texture for people.
pub fn name(texture: &str) -> String {
    match texture.strip_prefix(OWN) {
        Some(id) => {
            list().into_iter().find(|own| own.id == id).map_or_else(|| tr("Eigene", "Own").into(), |own| own.name)
        }
        None => match shipped(texture) {
            Some(s) => tr(s.de, s.en).into(),
            None => pattern_label(Pattern::from_id(texture).unwrap_or(Pattern::Knurl)).into(),
        },
    }
}
