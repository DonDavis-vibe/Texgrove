//! From a model and a texture to the textured model: refine, find the directions, move.

use crate::displace::{Relief, displace};
use crate::heightmap::Heightmap;
use crate::mesh::Mesh;
use crate::normals::{Weighting, face_normals, sharp_edges, vertex_normals};
use crate::project::{Mapping, Placement, Projector};
use crate::subdivide::{Limits, edge_for_budget, subdivide};
use crate::topology::Topology;

/// Everything that decides how a texture goes onto a model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub mapping: Mapping,
    pub placement: Placement,
    pub relief: Relief,
    /// The longest edge the refined mesh may have, in mm: the detail the texture can show.
    pub max_edge: f32,
    /// The most triangles the mesh may have while it is refined: a guard against running out of
    /// memory, not the size of the result. The edge length grows to fit, in steps (edges are
    /// halved or not), so the count can end up well below.
    pub max_triangles: usize,
    /// Hold the model's sharp edges in place, so they stay straight and the part keeps its
    /// measures there; the texture fades out towards them.
    pub protect_edges: bool,
    /// Edges whose faces differ by more than this many degrees count as sharp.
    pub sharp_degrees: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            mapping: Mapping::default(),
            placement: Placement::default(),
            relief: Relief::default(),
            max_edge: 0.5,
            max_triangles: 10_000_000,
            protect_edges: true,
            sharp_degrees: 30.0,
        }
    }
}

/// A textured model.
#[derive(Debug, Clone, Default)]
pub struct Textured {
    pub mesh: Mesh,
    /// For every triangle: the triangle of the source it is a piece of.
    pub parent: Vec<u32>,
    /// The edge length the mesh was refined to: `max_edge`, or more if the budget asked for it.
    pub edge: f32,
    /// Whether the budget still cut the refining short.
    pub capped: bool,
}

/// Puts a texture onto a welded mesh ([`crate::weld`]).
pub fn texture(source: &Mesh, map: &Heightmap, settings: &Settings) -> Textured {
    let bounds = source.bounds();
    let edge = edge_for_budget(source, settings.max_edge, settings.max_triangles, None);
    // The estimate may be off by some; the hard stop is well above the budget.
    let limits = Limits { max_edge: edge, max_triangles: settings.max_triangles.saturating_mul(2), refine: None };
    let refined = subdivide(source, limits);
    let mut mesh = refined.mesh;
    let normals = vertex_normals(&mesh, Weighting::Angle);
    let weights = settings.protect_edges.then(|| {
        let topology = Topology::build(&mesh);
        let sharp = sharp_edges(&mesh, &topology, &face_normals(&mesh), settings.sharp_degrees);
        let mut weights = vec![1.0f32; mesh.positions.len()];
        for (edge, _) in sharp.iter().enumerate().filter(|(_, sharp)| **sharp) {
            for vertex in topology.edge_vertices(&mesh, edge as u32) {
                weights[vertex as usize] = 0.0;
            }
        }
        weights
    });
    let projector = Projector::new(settings.mapping, &settings.placement, &bounds, map.aspect);
    displace(&mut mesh, &normals, weights.as_deref(), &projector, map, &settings.relief);
    Textured { mesh, parent: refined.parent, edge, capped: refined.capped }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::diagnose;
    use crate::pattern::Pattern;
    use crate::shapes;

    fn settings() -> Settings {
        Settings {
            placement: Placement { tile: 4.0, ..Placement::default() },
            relief: Relief { depth: 0.6, midlevel: 0.5, invert: false },
            max_edge: 0.5,
            ..Settings::default()
        }
    }

    #[test]
    fn a_textured_cube_is_still_closed_and_keeps_its_edges() {
        let cube = shapes::cuboid([20.0; 3]);
        let map = Pattern::Knurl.render(256);
        let textured = texture(&cube, &map, &settings());
        let report = diagnose(&textured.mesh);
        assert!(report.sound(), "{report:?}");
        assert_eq!((report.shells, report.euler(), report.flat_triangles), (1, 2, 0));
        // The relief goes in and out by 0.3 mm, but the edges are held: the box does not grow.
        let size = report.bounds.size();
        assert!(size.iter().all(|s| (s - 20.6).abs() < 0.05), "{size:?}");
        let corners = textured.mesh.positions.iter().filter(|p| p.iter().all(|c| *c == 0.0 || *c == 20.0)).count();
        assert_eq!(corners, 8);
        // A knurl's pyramids are a third high on average, a sixth below the middle level: the
        // 2400 mm² of surface sink by 0.6 mm / 6 on average.
        let expected = 8000.0 - 2400.0 * 0.6 / 6.0;
        assert!((report.volume - expected).abs() < 8000.0 * 0.005, "{} for {expected}", report.volume);
        assert!(report.area > 2400.0 * 1.01, "{}", report.area);
        assert!(!textured.capped && textured.edge == 0.5);
        assert_eq!(textured.parent.len(), textured.mesh.triangles.len());
    }

    #[test]
    fn without_edge_protection_the_edges_move_too() {
        let cube = shapes::cuboid([20.0; 3]);
        let map = Heightmap::new(1, 1, vec![1.0]).unwrap();
        let raise = Settings {
            relief: Relief { depth: 1.0, midlevel: 0.0, invert: false },
            protect_edges: false,
            max_edge: 2.0,
            ..Settings::default()
        };
        let grown = texture(&cube, &map, &raise);
        let report = diagnose(&grown.mesh);
        assert!(report.sound());
        // Everything moved out by 1 mm along its normal: faces straight, corners along the diagonal.
        let size = report.bounds.size();
        assert!(size.iter().all(|s| (s - 22.0).abs() < 1e-3), "{size:?}");
        let far = grown.mesh.positions.iter().map(|p| p[0].max(p[1]).max(p[2])).fold(0.0, f32::max);
        assert!((far - 21.0).abs() < 1e-3);
    }

    #[test]
    fn round_shapes_open_shapes_and_budgets() {
        let map = Pattern::Hexagons.render(256);
        let ring = shapes::torus(20.0, 6.0, 48, 24);
        let textured = texture(&ring, &map, &Settings { protect_edges: false, ..settings() });
        let report = diagnose(&textured.mesh);
        assert!(report.sound() && report.euler() == 0, "{report:?}");

        let sheet = shapes::sheet([30.0, 20.0], [3, 2]);
        let textured = texture(&sheet, &map, &settings());
        let report = diagnose(&textured.mesh);
        assert_eq!((report.non_manifold_edges, report.disagreeing_edges, report.euler()), (0, 0, 1));
        // The rim is an edge without a second face: it is held, so the outline stays.
        assert_eq!((report.bounds.min[0], report.bounds.max[0]), (0.0, 30.0));

        let small = texture(&ring, &map, &Settings { max_triangles: 20_000, protect_edges: false, ..settings() });
        assert!(small.edge > 0.5 && small.mesh.triangles.len() < 40_000, "{}", small.mesh.triangles.len());
    }

    #[test]
    fn the_same_input_gives_the_same_mesh() {
        let ball = shapes::sphere(10.0, 24, 12);
        let map = Pattern::Voronoi.render(128);
        let one = texture(&ball, &map, &settings());
        let two = texture(&ball, &map, &settings());
        assert_eq!(one.mesh, two.mesh);
    }
}
