//! What the user set in the panel, in a form that goes into a snapshot and into a job's
//! parameters, and its translation into the kernel's settings.

use serde::{Deserialize, Serialize};
use texgrove_core::Mesh;
use texgrove_core::bvh::Bvh;
use texgrove_core::displace::Relief;
use texgrove_core::heightmap::Heightmap;
use texgrove_core::mask::{EXCLUDED, Mask};
use texgrove_core::pattern::Pattern;
use texgrove_core::pipeline::{Finish, Settings};
use texgrove_core::project::{Mapping, Placement};
use texgrove_core::quality::{self, Quality};
use texgrove_core::select;
use texgrove_core::topology::Topology;

use crate::lang::tr;

/// How a texture goes onto a model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Recipe {
    /// A built-in pattern's id, or `own:<id>` for one of the user's pictures.
    pub texture: String,
    /// Blurs the texture over this part of a tile (percent): softens a photo's grain.
    pub smooth: f32,
    pub mapping: MappingChoice,
    /// The width of one tile in mm.
    pub tile: f32,
    /// From the texture's lowest to its highest point, in mm.
    pub depth: f32,
    pub relief: ReliefChoice,
    pub invert: bool,
    /// Degrees.
    pub rotation: f32,
    pub place: Place,
    /// Faces looking down within this many degrees stay smooth.
    pub bottom: f32,
    /// Faces looking up within this many degrees stay smooth.
    pub top: f32,
    pub quality: QualityChoice,
    pub format: Format,
}

impl Default for Recipe {
    fn default() -> Self {
        Recipe {
            texture: "knurl".into(),
            smooth: 0.0,
            mapping: MappingChoice::Triplanar,
            tile: 5.0,
            depth: 0.6,
            relief: ReliefChoice::Both,
            invert: false,
            rotation: 0.0,
            place: Place::All,
            bottom: 5.0,
            top: 0.0,
            quality: QualityChoice::Normal,
            format: Format::ThreeMf,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MappingChoice {
    #[default]
    Triplanar,
    Cubic,
    Cylindrical,
    Spherical,
    Xy,
    Xz,
    Yz,
}

impl MappingChoice {
    pub const ALL: [MappingChoice; 7] = [
        MappingChoice::Triplanar,
        MappingChoice::Cubic,
        MappingChoice::Cylindrical,
        MappingChoice::Spherical,
        MappingChoice::Xy,
        MappingChoice::Xz,
        MappingChoice::Yz,
    ];

    pub fn id(self) -> &'static str {
        match self {
            MappingChoice::Triplanar => "triplanar",
            MappingChoice::Cubic => "cubic",
            MappingChoice::Cylindrical => "cylindrical",
            MappingChoice::Spherical => "spherical",
            MappingChoice::Xy => "xy",
            MappingChoice::Xz => "xz",
            MappingChoice::Yz => "yz",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MappingChoice::Triplanar => tr("Rundum (triplanar)", "All round (triplanar)"),
            MappingChoice::Cubic => tr("Würfel", "Box"),
            MappingChoice::Cylindrical => tr("Zylinder", "Cylinder"),
            MappingChoice::Spherical => tr("Kugel", "Sphere"),
            MappingChoice::Xy => tr("Von oben", "From above"),
            MappingChoice::Xz => tr("Von vorn", "From the front"),
            MappingChoice::Yz => tr("Von der Seite", "From the side"),
        }
    }

    pub fn from_id(id: &str) -> Option<MappingChoice> {
        MappingChoice::ALL.into_iter().find(|m| m.id() == id)
    }

    fn mapping(self) -> Mapping {
        match self {
            MappingChoice::Triplanar => Mapping::default(),
            MappingChoice::Cubic => Mapping::Cubic,
            MappingChoice::Cylindrical => Mapping::Cylindrical { cap_degrees: 30.0 },
            MappingChoice::Spherical => Mapping::Spherical,
            MappingChoice::Xy => Mapping::PlanarXY,
            MappingChoice::Xz => Mapping::PlanarXZ,
            MappingChoice::Yz => Mapping::PlanarYZ,
        }
    }
}

/// Which way the relief goes from the surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReliefChoice {
    /// Bright parts out, dark parts in: the part keeps its volume.
    #[default]
    Both,
    /// Only outwards.
    Raise,
    /// Only inwards: the part keeps its outline.
    Sink,
}

impl ReliefChoice {
    pub const ALL: [ReliefChoice; 3] = [ReliefChoice::Both, ReliefChoice::Raise, ReliefChoice::Sink];

    pub fn id(self) -> &'static str {
        match self {
            ReliefChoice::Both => "both",
            ReliefChoice::Raise => "raise",
            ReliefChoice::Sink => "sink",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ReliefChoice::Both => tr("Rein und raus", "In and out"),
            ReliefChoice::Raise => tr("Nur raus", "Only out"),
            ReliefChoice::Sink => tr("Nur rein", "Only in"),
        }
    }

    pub fn from_id(id: &str) -> Option<ReliefChoice> {
        ReliefChoice::ALL.into_iter().find(|r| r.id() == id)
    }
}

/// Where on the model the texture goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    #[default]
    All,
    Outside,
    Inside,
}

impl Place {
    pub const ALL: [Place; 3] = [Place::All, Place::Outside, Place::Inside];

    pub fn id(self) -> &'static str {
        match self {
            Place::All => "all",
            Place::Outside => "outside",
            Place::Inside => "inside",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Place::All => tr("Überall", "Everywhere"),
            Place::Outside => tr("Nur außen", "Outside only"),
            Place::Inside => tr("Nur innen", "Inside only"),
        }
    }

    pub fn from_id(id: &str) -> Option<Place> {
        Place::ALL.into_iter().find(|p| p.id() == id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum QualityChoice {
    Draft,
    #[default]
    Normal,
    Fine,
}

impl QualityChoice {
    pub const ALL: [QualityChoice; 3] = [QualityChoice::Draft, QualityChoice::Normal, QualityChoice::Fine];

    pub fn id(self) -> &'static str {
        match self {
            QualityChoice::Draft => "draft",
            QualityChoice::Normal => "normal",
            QualityChoice::Fine => "fine",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            QualityChoice::Draft => tr("Entwurf", "Draft"),
            QualityChoice::Normal => tr("Normal", "Normal"),
            QualityChoice::Fine => tr("Fein", "Fine"),
        }
    }

    pub fn from_id(id: &str) -> Option<QualityChoice> {
        QualityChoice::ALL.into_iter().find(|q| q.id() == id)
    }

    fn quality(self) -> Quality {
        match self {
            QualityChoice::Draft => Quality::Draft,
            QualityChoice::Normal => Quality::Normal,
            QualityChoice::Fine => Quality::Fine,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    #[default]
    #[serde(rename = "3mf")]
    ThreeMf,
    Stl,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::ThreeMf => "3mf",
            Format::Stl => "stl",
        }
    }
}

/// A pattern's name for people.
pub fn pattern_label(pattern: Pattern) -> &'static str {
    match pattern {
        Pattern::Knurl => tr("Rändel", "Knurl"),
        Pattern::Grooves => tr("Rillen", "Grooves"),
        Pattern::Ribs => tr("Rippen", "Ribs"),
        Pattern::Stripes => tr("Streifen", "Stripes"),
        Pattern::Dots => tr("Noppen", "Dots"),
        Pattern::Hexagons => tr("Waben", "Hexagons"),
        Pattern::Voronoi => tr("Zellen", "Cells"),
        Pattern::Noise => tr("Rauschen", "Noise"),
        Pattern::Bricks => tr("Ziegel", "Bricks"),
        Pattern::Weave => tr("Geflecht", "Weave"),
    }
}

impl Recipe {
    /// The built-in pattern, if the texture is one.
    pub fn pattern(&self) -> Option<Pattern> {
        Pattern::from_id(&self.texture)
    }

    /// The height map of the texture, smoothed as asked.
    pub fn map(&self) -> Result<Heightmap, String> {
        let map = crate::textures::map(&self.texture)?;
        let radius = self.smooth.clamp(0.0, 25.0) / 100.0 * map.width() as f32;
        Ok(if radius >= 0.5 { map.blurred(radius, texgrove_core::heightmap::Tiling::Repeat) } else { map })
    }

    /// The size of the finest thing the texture shows, in mm.
    pub fn feature(&self, map: &Heightmap) -> f32 {
        let detail = match self.pattern() {
            Some(pattern) if self.smooth <= 0.0 => pattern.detail(),
            _ => map.detail(),
        };
        self.tile.max(0.1) * detail
    }

    fn relief(&self) -> Relief {
        let midlevel = match self.relief {
            ReliefChoice::Both => 0.5,
            ReliefChoice::Raise => 0.0,
            ReliefChoice::Sink => 1.0,
        };
        Relief { depth: self.depth.max(0.0), midlevel, invert: self.invert }
    }

    /// Where on `source` (a welded mesh) the texture goes.
    pub fn mask(&self, source: &Mesh) -> Mask {
        let mut mask = Mask { bottom_degrees: self.bottom, top_degrees: self.top, ..Mask::default() };
        if self.place != Place::All {
            let inside = select::inside(source, &Topology::build(source), &Bvh::build(source), 30.0);
            let wanted = self.place == Place::Inside;
            mask.faces = inside.iter().map(|inside| if *inside == wanted { 0 } else { EXCLUDED }).collect();
        }
        mask
    }

    /// The kernel's settings for the file to keep: the quality's fineness for this nozzle.
    pub fn settings(&self, source: &Mesh, nozzle: f32, map: &Heightmap) -> Settings {
        let finish = quality::finish(self.quality.quality(), self.feature(map), nozzle);
        self.with_finish(source, finish)
    }

    /// The kernel's settings for a quick look: at most about `budget` triangles, nothing reduced.
    pub fn preview_settings(&self, source: &Mesh, nozzle: f32, budget: usize, map: &Heightmap) -> Settings {
        let draft = quality::finish(Quality::Draft, self.feature(map), nozzle);
        let finish = Finish { max_triangles: budget, triangles: usize::MAX, tolerance: 0.0, ..draft };
        self.with_finish(source, finish)
    }

    fn with_finish(&self, source: &Mesh, finish: Finish) -> Settings {
        Settings {
            mapping: self.mapping.mapping(),
            placement: Placement { tile: self.tile.max(0.1), rotation: self.rotation, ..Placement::default() },
            relief: self.relief(),
            mask: self.mask(source),
            finish,
        }
    }

    /// What the settings were, in words, for the note on the file.
    pub fn describe(&self) -> String {
        use crate::lang::number;
        let mut parts = vec![
            crate::textures::name(&self.texture),
            crate::tf!("Kachel {} mm", "tile {} mm", number(self.tile, 1)),
            crate::tf!("Tiefe {} mm", "depth {} mm", number(self.depth, 2)),
            self.relief.label().to_lowercase(),
            self.mapping.label().to_owned(),
            self.place.label().to_lowercase(),
        ];
        if self.rotation != 0.0 {
            parts.push(crate::tf!("gedreht um {}°", "turned by {}°", number(self.rotation, 0)));
        }
        if self.smooth > 0.0 {
            parts.push(crate::tf!("geglättet {} %", "smoothed {} %", number(self.smooth, 1)));
        }
        if self.invert {
            parts.push(tr("umgekehrt", "inverted").into());
        }
        if self.bottom > 0.0 {
            parts.push(crate::tf!("Unterseite frei ({}°)", "bottom left smooth ({}°)", number(self.bottom, 0)));
        }
        if self.top > 0.0 {
            parts.push(crate::tf!("Oberseite frei ({}°)", "top left smooth ({}°)", number(self.top, 0)));
        }
        parts.push(crate::tf!("Qualität {}", "quality {}", self.quality.label()));
        parts.join(", ")
    }
}
