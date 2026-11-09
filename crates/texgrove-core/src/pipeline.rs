//! From a model and its textures to the textured model: refine, find the directions, move, reduce.

use crate::bvh::Bvh;
use crate::decimate::{Target, decimate};
use crate::displace::{Blend, Coat, Relief, lay_on, shift};
use crate::heightmap::Heightmap;
use crate::mask::Mask;
use crate::mesh::Mesh;
use crate::normals::{Weighting, face_normals, sharp_edges, vertex_normals};
use crate::project::{Mapping, Placement, Projector};
use crate::subdivide::{Limits, edge_for_budget, subdivide};
use crate::topology::Topology;
use crate::vec::{self, P3};

/// One texture on a model: what, how it is laid on, how deep, and where.
#[derive(Debug, Clone)]
pub struct Layer<'a> {
    pub map: &'a Heightmap,
    pub mapping: Mapping,
    pub placement: Placement,
    pub relief: Relief,
    pub mask: Mask,
    /// How it meets the layers before it in the list.
    pub blend: Blend,
}

/// How fine the mesh is made for the textures and how small it is afterwards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Finish {
    /// The longest edge the refined mesh may have, in mm: the detail a texture can show.
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
    /// The most triangles the result may have: the textured mesh is reduced to it, taking away
    /// first what changes the shape least. `usize::MAX` keeps everything.
    pub triangles: usize,
    /// Below that, what moves the surface by less than this (mm) is still taken away: flat
    /// stretches that were cut fine for nothing. 0 takes away nothing more.
    pub tolerance: f32,
}

impl Default for Finish {
    fn default() -> Self {
        Finish {
            max_edge: 0.5,
            max_triangles: 10_000_000,
            protect_edges: true,
            sharp_degrees: 30.0,
            triangles: 1_000_000,
            tolerance: 0.005,
        }
    }
}

/// Everything that decides how one texture goes onto a model: a [`Layer`] without its height
/// map, and the [`Finish`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Settings {
    pub mapping: Mapping,
    pub placement: Placement,
    pub relief: Relief,
    pub mask: Mask,
    pub finish: Finish,
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
    /// How many triangles the mesh had before it was reduced.
    pub refined_triangles: usize,
    /// What the user should know about the result.
    pub warnings: Vec<Warning>,
    /// For every triangle whether it got no texture at all (left out by a mask, or beyond a
    /// stamp): to show where the texture goes.
    pub left_out: Vec<bool>,
}

/// Something about a result worth a look before printing.
#[derive(Debug, Clone, PartialEq)]
pub enum Warning {
    /// The relief goes deeper than a tenth of the part's smallest size: walls may meet or
    /// break through.
    TooDeep { depth: f32, smallest: f32 },
    /// Triangles that would have turned over where the relief is steep for the mesh; their
    /// relief was flattened there.
    Flattened { triangles: usize },
}

/// What the pipeline is doing, for a progress bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Refine,
    Move,
    Reduce,
}

/// Puts one texture onto a welded mesh ([`crate::weld`]).
pub fn texture(source: &Mesh, map: &Heightmap, settings: &Settings) -> Textured {
    texture_with(source, map, settings, &mut |_, _| true).expect("nothing stops it")
}

/// [`texture`], telling `go_on` what it is doing and how far it is (0 to 1 within a stage).
/// `go_on` stops the work by answering `false`; then there is no result.
pub fn texture_with(
    source: &Mesh,
    map: &Heightmap,
    settings: &Settings,
    go_on: &mut dyn FnMut(Stage, f32) -> bool,
) -> Option<Textured> {
    let layer = Layer {
        map,
        mapping: settings.mapping,
        placement: settings.placement,
        relief: settings.relief,
        mask: settings.mask.clone(),
        blend: Blend::Cover,
    };
    texture_layers(source, &[layer], &settings.finish, go_on)
}

/// Puts several textures onto a welded mesh, one over the other in the order given.
pub fn texture_layers(
    source: &Mesh,
    layers: &[Layer],
    finish: &Finish,
    go_on: &mut dyn FnMut(Stage, f32) -> bool,
) -> Option<Textured> {
    if !go_on(Stage::Refine, 0.0) {
        return None;
    }
    let bounds = source.bounds();
    // What each layer may touch, and so what is worth refining: what any of them may.
    let active: Vec<Vec<bool>> = layers.iter().map(|layer| layer.mask.active(source)).collect();
    let any: Vec<bool> = (0..source.triangles.len()).map(|t| active.iter().any(|a| a[t])).collect();
    let edge = edge_for_budget(source, finish.max_edge, finish.max_triangles, Some(&any));
    let limits = Limits { max_edge: edge, max_triangles: finish.max_triangles, refine: Some(&any) };
    let refined = subdivide(source, limits);
    if !go_on(Stage::Move, 0.0) {
        return None;
    }
    let mut mesh = refined.mesh;
    let mut parent = refined.parent;
    let normals = vertex_normals(&mesh, Weighting::Angle);
    let held = finish.protect_edges.then(|| {
        let topology = Topology::build(&mesh);
        let sharp = sharp_edges(&mesh, &topology, &face_normals(&mesh), finish.sharp_degrees);
        let mut held = vec![false; mesh.positions.len()];
        for (edge, _) in sharp.iter().enumerate().filter(|(_, sharp)| **sharp) {
            for vertex in topology.edge_vertices(&mesh, edge as u32) {
                held[vertex as usize] = true;
            }
        }
        held
    });
    // The tree is only for brush strokes; most models have none.
    let stamps = layers.iter().any(|layer| matches!(layer.mapping, Mapping::Stamp { .. }));
    let bvh = (stamps || layers.iter().any(|layer| !layer.mask.dabs.is_empty())).then(|| Bvh::build(&mesh));
    let mut offsets = vec![0.0f32; mesh.positions.len()];
    // The most any layer puts on each vertex.
    let mut covered = vec![0.0f32; mesh.positions.len()];
    for (i, layer) in layers.iter().enumerate() {
        if !go_on(Stage::Move, i as f32 / layers.len() as f32) {
            return None;
        }
        let mut weights = layer.mask.weights(&active[i], &mesh, &parent, &normals, held.as_deref(), bvh.as_ref());
        let projector = Projector::for_mesh(layer.mapping, &layer.placement, source, layer.map.aspect);
        let reach = bounds.diagonal().max(1e-3);
        for (v, weight) in weights.iter_mut().enumerate() {
            if *weight <= 0.0 {
                continue;
            }
            *weight *= projector.cover(mesh.positions[v], normals[v]);
            // A stamp reaches only what it can see: nothing of the part in between.
            if let (Mapping::Stamp { normal: facing, .. }, Some(bvh)) = (layer.mapping, bvh.as_ref())
                && *weight > 0.0
            {
                let origin = vec::add(mesh.positions[v], vec::scale(normals[v], reach * 1e-4));
                if bvh.blocked(&mesh, origin, vec::normalize(facing), 0.0, f32::INFINITY) {
                    *weight = 0.0;
                }
            }
        }
        for (most, weight) in covered.iter_mut().zip(&weights) {
            *most = most.max(*weight);
        }
        let coat = Coat { projector: &projector, map: layer.map, relief: &layer.relief, blend: layer.blend };
        lay_on(&mut offsets, &mesh, &normals, Some(&weights), &coat);
    }
    let mut warnings = Vec::new();
    let deepest = offsets.iter().fold(0.0f32, |m, o| m.max(o.abs()));
    let size = bounds.size();
    let smallest = size[0].min(size[1]).min(size[2]);
    if smallest > 0.0 && deepest > 0.1 * smallest {
        warnings.push(Warning::TooDeep { depth: deepest, smallest });
    }
    let flattened = guard_flips(&mut mesh, &normals, &mut offsets);
    if flattened > 0 {
        warnings.push(Warning::Flattened { triangles: flattened });
    }
    let mut left_out: Vec<bool> = mesh
        .triangles
        .iter()
        .enumerate()
        .map(|(t, corners)| {
            // A triangle whose corners all sit on held edges moves nothing either way: it goes
            // with the face it is a piece of.
            let all_held = held.as_ref().is_some_and(|held| corners.iter().all(|&v| held[v as usize]));
            !any[parent[t] as usize] || (!all_held && corners.iter().all(|&v| covered[v as usize] <= 0.0))
        })
        .collect();
    drop((normals, offsets, held, bvh, covered));
    let refined_triangles = mesh.triangles.len();
    if !go_on(Stage::Reduce, 0.0) {
        return None;
    }
    if refined_triangles > finish.triangles || finish.tolerance > 0.0 {
        let target = Target { triangles: finish.triangles, tolerance: finish.tolerance };
        let reduced = decimate(&mesh, target, |done| go_on(Stage::Reduce, done));
        if reduced.stopped {
            return None;
        }
        parent = reduced.source_triangle.iter().map(|&t| parent[t as usize]).collect();
        left_out = reduced.source_triangle.iter().map(|&t| left_out[t as usize]).collect();
        mesh = reduced.mesh;
    }
    Some(Textured { mesh, parent, edge, capped: refined.capped, refined_triangles, warnings, left_out })
}

/// Moves the vertices by their offsets, but where a triangle would turn over (the relief steeper
/// than the mesh can follow), halves the offsets of its corners and tries again, three times at
/// most. How many triangles needed it.
fn guard_flips(mesh: &mut Mesh, normals: &[P3], offsets: &mut [f32]) -> usize {
    let before: Vec<P3> = face_normals(mesh);
    let start = mesh.positions.clone();
    let mut touched = vec![false; mesh.triangles.len()];
    for round in 0..4 {
        mesh.positions.copy_from_slice(&start);
        shift(mesh, normals, offsets);
        if round == 3 {
            break;
        }
        let mut flipped = false;
        for t in 0..mesh.triangles.len() {
            let now = crate::normals::face_normal(mesh, t);
            if vec::dot(now, before[t]) < 0.0 || (now == [0.0; 3] && before[t] != [0.0; 3]) {
                flipped = true;
                touched[t] = true;
                for v in mesh.triangles[t] {
                    offsets[v as usize] *= 0.5;
                }
            }
        }
        if !flipped {
            break;
        }
    }
    touched.iter().filter(|t| **t).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::diagnose;
    use crate::mask::{Dab, DabKind, EXCLUDED, INCLUDED};
    use crate::pattern::Pattern;
    use crate::select;
    use crate::shapes;

    /// Everything kept: no reducing, so the tests see what refining and moving did.
    fn keep_all() -> Finish {
        Finish { max_edge: 0.5, triangles: usize::MAX, tolerance: 0.0, ..Finish::default() }
    }

    fn settings() -> Settings {
        Settings {
            placement: Placement { tile: 4.0, ..Placement::default() },
            relief: Relief { depth: 0.6, midlevel: 0.5, invert: false },
            finish: keep_all(),
            ..Settings::default()
        }
    }

    fn free_edges() -> Settings {
        Settings { finish: Finish { protect_edges: false, ..keep_all() }, ..settings() }
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
            finish: Finish { max_edge: 2.0, ..free_edges().finish },
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
        let textured = texture(&ring, &map, &free_edges());
        let report = diagnose(&textured.mesh);
        assert!(report.sound() && report.euler() == 0, "{report:?}");

        let sheet = shapes::sheet([30.0, 20.0], [3, 2]);
        let textured = texture(&sheet, &map, &settings());
        let report = diagnose(&textured.mesh);
        assert_eq!((report.non_manifold_edges, report.disagreeing_edges, report.euler()), (0, 0, 1));
        // The rim is an edge without a second face: it is held, so the outline stays.
        assert_eq!((report.bounds.min[0], report.bounds.max[0]), (0.0, 30.0));

        let tight = Settings { finish: Finish { max_triangles: 20_000, ..free_edges().finish }, ..settings() };
        let small = texture(&ring, &map, &tight);
        assert!(small.edge > 0.5 && small.mesh.triangles.len() <= 20_000, "{}", small.mesh.triangles.len());
    }

    #[test]
    fn the_result_is_reduced_to_its_budget_and_can_be_stopped() {
        let cube = shapes::cuboid([20.0; 3]);
        let map = Pattern::Knurl.render(256);
        let full = texture(&cube, &map, &settings());
        let lean = texture(&cube, &map, &Settings { finish: Finish { triangles: 20_000, ..keep_all() }, ..settings() });
        assert_eq!(lean.refined_triangles, full.mesh.triangles.len());
        assert!(lean.mesh.triangles.len() <= 20_000);
        assert_eq!(lean.parent.len(), lean.mesh.triangles.len());
        assert!(lean.parent.iter().all(|&p| p < 12));
        let (a, b) = (diagnose(&lean.mesh), diagnose(&full.mesh));
        assert!(a.sound() && (a.volume - b.volume).abs() < b.volume * 0.003, "{} {}", a.volume, b.volume);

        // An untextured stretch (black, nothing raised) goes back to a few triangles by itself.
        let black = Heightmap::new(1, 1, vec![0.0]).unwrap();
        let raise = Settings {
            relief: Relief { depth: 1.0, midlevel: 0.0, invert: false },
            finish: Finish { tolerance: 0.001, ..keep_all() },
            ..settings()
        };
        let plain = texture(&cube, &black, &raise);
        assert!(plain.mesh.triangles.len() < 100 && plain.refined_triangles > 10_000, "{}", plain.mesh.triangles.len());

        let mut stages = Vec::new();
        let stopped = texture_with(&cube, &map, &settings(), &mut |stage, _| {
            stages.push(stage);
            stage != Stage::Reduce
        });
        assert!(stopped.is_none());
        assert_eq!(stages, vec![Stage::Refine, Stage::Move, Stage::Move, Stage::Reduce]);
    }

    #[test]
    fn the_same_input_gives_the_same_mesh() {
        let ball = shapes::sphere(10.0, 24, 12);
        let map = Pattern::Voronoi.render(128);
        let one = texture(&ball, &map, &settings());
        let two = texture(&ball, &map, &settings());
        assert_eq!(one.mesh, two.mesh);
    }

    #[test]
    fn a_part_keeps_lying_flat_when_its_bottom_is_left_out() {
        let cube = shapes::cuboid([20.0; 3]);
        let map = Pattern::Bricks.render(256);
        let flat = Settings { mask: Mask::without_bottom(5.0), ..settings() };
        let textured = texture(&cube, &map, &flat);
        let report = diagnose(&textured.mesh);
        assert!(report.sound(), "{report:?}");
        // The triangles of the bottom, and only they, are told apart as left out.
        assert_eq!(textured.left_out.len(), textured.mesh.triangles.len());
        for (t, out) in textured.left_out.iter().enumerate() {
            assert_eq!(*out, textured.parent[t] < 2, "triangle {t} of source {}", textured.parent[t]);
        }
        // Nothing reaches below the bed, and the foot is the full square it was.
        assert_eq!(report.bounds.min[2], 0.0);
        let on_bed: Vec<_> = textured.mesh.positions.iter().filter(|p| p[2] == 0.0).collect();
        assert!(
            on_bed.iter().any(|p| p[0] == 0.0 && p[1] == 0.0) && on_bed.iter().any(|p| p[0] == 20.0 && p[1] == 20.0)
        );
        // The bottom's two triangles were only cut where the walls needed it, and the top moved.
        assert!(textured.parent.iter().filter(|&&p| p < 2).count() < 400);
        assert!(report.bounds.max[2] > 20.1);
    }

    /// A tube standing on z = 0: per segment two triangles each of outer wall, top, bore, bottom.
    fn tube() -> Mesh {
        let (outer, inner, height, segments) = (20.0f32, 15.0f32, 40.0f32, 64u32);
        let mut mesh = Mesh::default();
        for s in 0..segments {
            let turn = core::f32::consts::TAU * s as f32 / segments as f32;
            for (radius, z) in [(outer, 0.0), (outer, height), (inner, height), (inner, 0.0)] {
                mesh.positions.push([radius * libm::cosf(turn), radius * libm::sinf(turn), z]);
            }
        }
        for s in 0..segments {
            let next = (s + 1) % segments;
            for k in 0..4 {
                let (a, b) = (s * 4 + k, s * 4 + (k + 1) % 4);
                let (c, d) = (next * 4 + (k + 1) % 4, next * 4 + k);
                mesh.triangles.push([a, d, c]);
                mesh.triangles.push([a, c, b]);
            }
        }
        mesh
    }

    #[test]
    fn only_the_outside_of_a_tube_is_textured_and_its_bore_still_fits() {
        let tube = tube();
        let map = Pattern::Knurl.render(256);
        // Mark what does not lie open, and the flat ends, as staying.
        let inside = select::inside(&tube, &Topology::build(&tube), &Bvh::build(&tube), 30.0);
        let faces = inside.iter().map(|inside| if *inside { EXCLUDED } else { 0 }).collect();
        let outside_only = Settings {
            mapping: Mapping::Cylindrical { cap_degrees: 30.0, cap_blend: 0.0 },
            mask: Mask { faces, top_degrees: 5.0, bottom_degrees: 5.0, ..Mask::default() },
            relief: Relief { depth: 1.0, midlevel: 0.0, invert: false },
            ..settings()
        };
        let textured = texture(&tube, &map, &outside_only);
        let report = diagnose(&textured.mesh);
        assert!(report.sound() && report.euler() == 0, "{report:?}");
        let radius = |p: &[f32; 3]| libm::sqrtf(p[0] * p[0] + p[1] * p[1]);
        // Nothing came nearer to the axis than the bore was, and the ends are as flat as before.
        let nearest = textured.mesh.positions.iter().map(radius).fold(f32::INFINITY, f32::min);
        assert!(nearest > 15.0 * libm::cosf(core::f32::consts::PI / 64.0) - 1e-3, "{nearest}");
        assert_eq!((report.bounds.min[2], report.bounds.max[2]), (0.0, 40.0));
        // The outside grew by up to the depth.
        let farthest = textured.mesh.positions.iter().map(radius).fold(0.0, f32::max);
        assert!(farthest > 20.8 && farthest <= 21.0 + 1e-3, "{farthest}");
        // The bore was not even refined: two triangles per segment, as before.
        let bore = textured.parent.iter().filter(|&&p| (p / 2) % 4 == 2).count();
        assert!(bore < 64 * 2 * 4, "{bore}");
    }

    #[test]
    fn a_stamp_lands_once_where_it_was_put_and_only_there() {
        let cube = shapes::cuboid([30.0; 3]);
        let white = Heightmap::new(1, 1, vec![1.0]).unwrap();
        let stamp = Settings {
            mapping: Mapping::Stamp { centre: [15.0, 0.0, 15.0], normal: [0.0, -1.0, 0.0] },
            placement: Placement { tile: 10.0, ..Placement::default() },
            relief: Relief { depth: 1.0, midlevel: 0.0, invert: false },
            ..settings()
        };
        let stamped = texture(&cube, &white, &stamp);
        let report = diagnose(&stamped.mesh);
        assert!(report.sound(), "{report:?}");
        // A raised square on the front, 10 mm wide; the back, though in the stamp's line, stays.
        let raised: Vec<_> = stamped.mesh.positions.iter().filter(|p| p[1] < -0.5).collect();
        assert!(!raised.is_empty());
        assert!(raised.iter().all(|p| (p[0] - 15.0).abs() <= 5.0 && (p[2] - 15.0).abs() <= 5.0));
        assert!(stamped.mesh.positions.iter().all(|p| p[1] <= 30.0));
        assert!(stamped.warnings.is_empty(), "{:?}", stamped.warnings);
    }

    #[test]
    fn too_deep_a_relief_is_reported_and_turned_triangles_are_flattened() {
        let plate = shapes::cuboid([40.0, 40.0, 2.0]);
        let map = Pattern::Knurl.render(256);
        let deep = Settings { relief: Relief { depth: 1.5, midlevel: 0.5, invert: false }, ..free_edges() };
        let textured = texture(&plate, &map, &deep);
        assert!(textured.warnings.iter().any(|w| matches!(w, Warning::TooDeep { smallest, .. } if *smallest == 2.0)));
        // A relief far steeper than the mesh: some triangles had to be flattened, none turned.
        let coarse = Settings {
            relief: Relief { depth: 6.0, midlevel: 0.5, invert: false },
            placement: Placement { tile: 1.0, ..Placement::default() },
            finish: Finish { max_edge: 0.8, ..free_edges().finish },
            ..settings()
        };
        let steep = texture(&shapes::cuboid([20.0; 3]), &map, &coarse);
        assert!(steep.warnings.iter().any(|w| matches!(w, Warning::Flattened { .. })), "{:?}", steep.warnings);
        assert_eq!(diagnose(&steep.mesh).disagreeing_edges, 0);
    }

    #[test]
    fn layers_lie_over_each_other_and_a_brush_keeps_a_patch_clear() {
        let cube = shapes::cuboid([20.0; 3]);
        let (white, knurl) = (Heightmap::new(1, 1, vec![1.0]).unwrap(), Pattern::Knurl.render(256));
        let raise = Relief { depth: 1.0, midlevel: 0.0, invert: false };
        let mut top = vec![0; 12];
        top[2] = INCLUDED;
        top[3] = INCLUDED;
        let placement = Placement { tile: 4.0, ..Placement::default() };
        let base = Layer {
            map: &white,
            mapping: Mapping::default(),
            placement,
            relief: raise,
            mask: Mask { faces: top.clone(), ..Mask::default() },
            blend: Blend::Cover,
        };
        // A plate raised by 1 mm on the top: the cube is 21 high, its foot untouched.
        let plate = texture_layers(&cube, std::slice::from_ref(&base), &keep_all(), &mut |_, _| true).unwrap();
        let report = diagnose(&plate.mesh);
        assert!(report.sound());
        assert_eq!((report.bounds.min, report.bounds.max), ([0.0; 3], [20.0, 20.0, 21.0]));

        // A knurl added on the plate reaches higher; covering it instead does not.
        let pattern = Layer { map: &knurl, relief: Relief { depth: 0.5, ..raise }, ..base.clone() };
        let added = [base.clone(), Layer { blend: Blend::Add, ..pattern.clone() }];
        let added = texture_layers(&cube, &added, &keep_all(), &mut |_, _| true).unwrap();
        let top_of = |t: &Textured| t.mesh.bounds().max[2];
        assert!((top_of(&added) - 21.5).abs() < 0.02, "{}", top_of(&added));
        let covered = texture_layers(&cube, &[base.clone(), pattern], &keep_all(), &mut |_, _| true).unwrap();
        assert!((top_of(&covered) - 20.5).abs() < 0.02, "{}", top_of(&covered));
        assert!(diagnose(&added.mesh).sound() && diagnose(&covered.mesh).sound());

        // A brushed patch in the middle of the plate stays down.
        let dab = Dab {
            centre: [10.0, 10.0, 20.0],
            normal: [0.0, 0.0, 1.0],
            radius: 4.0,
            hardness: 1.0,
            kind: DabKind::Exclude,
            erase: false,
        };
        let holed = Layer { mask: Mask { faces: top, dabs: vec![dab], ..Mask::default() }, ..base };
        let holed = texture_layers(&cube, &[holed], &keep_all(), &mut |_, _| true).unwrap();
        for p in holed.mesh.positions.iter().filter(|p| p[2] >= 20.0) {
            let from_centre = libm::sqrtf((p[0] - 10.0).powi(2) + (p[1] - 10.0).powi(2));
            let inside = p[0] > 0.0 && p[0] < 20.0 && p[1] > 0.0 && p[1] < 20.0;
            assert_eq!(p[2], if from_centre <= 4.0 || !inside { 20.0 } else { 21.0 }, "{p:?}");
        }
        assert!(diagnose(&holed.mesh).sound());
    }
}
