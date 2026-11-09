//! Cutting triangles small enough to carry a texture.
//!
//! A texture can only show where the mesh has vertices to move. So every edge longer than a
//! chosen length is halved, again and again, until none is left. An edge is halved for all
//! triangles that share it at once, so the mesh stays as closed as it was. New vertices lie
//! exactly on the old edges: flat faces stay flat and straight edges straight.

use crate::hash::FastMap;
use crate::mesh::Mesh;
use crate::vec;

/// A mesh cut finer, and where its triangles came from.
#[derive(Debug, Clone, Default)]
pub struct Subdivided {
    pub mesh: Mesh,
    /// For every triangle: the triangle of the original mesh it is a piece of.
    pub parent: Vec<u32>,
    /// How many times all edges were looked through.
    pub passes: u32,
    /// Whether cutting stopped because the mesh reached `max_triangles`, with long edges left.
    pub capped: bool,
}

/// What to cut and how far.
#[derive(Debug, Clone, Copy)]
pub struct Limits<'a> {
    /// No edge of a triangle that is to be refined stays longer than this (mm).
    pub max_edge: f32,
    /// Stop once the mesh has this many triangles (it may overshoot by one pass's worth).
    pub max_triangles: usize,
    /// Which triangles of the mesh are to be refined; `None` for all. The others are only cut
    /// where they share a long edge with one that is, so no gap opens between them.
    pub refine: Option<&'a [bool]>,
}

/// Halves every too long edge of the triangles to refine until none is left.
pub fn subdivide(mesh: &Mesh, limits: Limits) -> Subdivided {
    let limit_squared = f64::from(limits.max_edge.max(1e-6)).powi(2);
    let mut out =
        Subdivided { mesh: mesh.clone(), parent: (0..mesh.triangles.len() as u32).collect(), passes: 0, capped: false };
    let refined = |parent: u32| limits.refine.is_none_or(|r| r.get(parent as usize).copied().unwrap_or(false));
    // Halving shrinks the longest edge by half each pass, so this ends; the bound is for meshes
    // with coordinates gone wild.
    for _ in 0..40 {
        if out.mesh.triangles.len() >= limits.max_triangles {
            out.capped = has_long_edge(&out, limit_squared, &refined);
            break;
        }
        // The middle of every edge to halve, by its two vertices.
        let mut middles: FastMap<u64, u32> = FastMap::default();
        for (t, corners) in out.mesh.triangles.iter().enumerate() {
            if !refined(out.parent[t]) {
                continue;
            }
            for k in 0..3 {
                let (a, b) = (corners[k], corners[(k + 1) % 3]);
                if distance_squared(&out.mesh, a, b) > limit_squared {
                    middles.entry(edge_key(a, b)).or_insert(u32::MAX);
                }
            }
        }
        if middles.is_empty() {
            break;
        }
        out.passes += 1;
        let count = out.mesh.triangles.len();
        let mut triangles = Vec::with_capacity(count + middles.len() * 2);
        let mut parent = Vec::with_capacity(count + middles.len() * 2);
        for t in 0..count {
            let corners = out.mesh.triangles[t];
            // The middle of each side, made when first asked for.
            let mut middle = [None; 3];
            for k in 0..3 {
                let (a, b) = (corners[k], corners[(k + 1) % 3]);
                if let Some(vertex) = middles.get_mut(&edge_key(a, b)) {
                    if *vertex == u32::MAX {
                        *vertex = out.mesh.positions.len() as u32;
                        let (pa, pb) = (out.mesh.positions[a as usize], out.mesh.positions[b as usize]);
                        // Summed in the order of the vertex numbers: the same from both sides.
                        let (first, second) = if a < b { (pa, pb) } else { (pb, pa) };
                        out.mesh.positions.push(vec::scale(vec::add(first, second), 0.5));
                    }
                    middle[k] = Some(*vertex);
                }
            }
            let before = triangles.len();
            split(&out.mesh, corners, middle, &mut triangles);
            parent.extend(std::iter::repeat_n(out.parent[t], triangles.len() - before));
        }
        out.mesh.triangles = triangles;
        out.parent = parent;
    }
    out
}

/// How many triangles [`subdivide`] will make of the mesh: for choosing an edge length that
/// fits a budget before doing the work. Counting stops once it passes `stop_at`.
///
/// Each triangle is cut in thought, by the lengths of its sides alone. What neighbours force on
/// a triangle that is not to be refined is left out, so the real count can be a little higher.
pub fn estimate(mesh: &Mesh, max_edge: f32, refine: Option<&[bool]>, stop_at: usize) -> usize {
    let limit = f64::from(max_edge.max(1e-6));
    let mut total = 0usize;
    for t in 0..mesh.triangles.len() {
        if refine.is_some_and(|r| !r.get(t).copied().unwrap_or(false)) {
            total += 1;
        } else {
            let [a, b, c] = mesh.triangles[t];
            let sides = [distance_squared(mesh, a, b), distance_squared(mesh, b, c), distance_squared(mesh, c, a)];
            total = total.saturating_add(pieces(sides.map(f64::sqrt), limit, 0));
        }
        if total > stop_at {
            break;
        }
    }
    total
}

/// Into how many pieces a triangle with these three sides (in the order of its corners) falls
/// when every side longer than `limit` is halved: the cuts of [`split`], on lengths alone.
fn pieces(sides: [f64; 3], limit: f64, depth: u32) -> usize {
    let long = sides.map(|s| s > limit);
    // The length of the line from the middle of side `x` to the corner opposite it.
    let median = |x: f64, y: f64, z: f64| 0.5 * (2.0 * y * y + 2.0 * z * z - x * x).max(0.0).sqrt();
    if depth >= 40 || !sides.iter().all(|s| s.is_finite()) {
        return 1;
    }
    match long {
        [false, false, false] => 1,
        // Four triangles of half the size, all alike.
        [true, true, true] => pieces(sides.map(|s| s / 2.0), limit, depth + 1).saturating_mul(4),
        _ => {
            // Turned as in `split`: a long first side, and a short third one.
            let turn = match long {
                [true, false, false] | [true, true, false] => 0,
                [false, true, false] | [false, true, true] => 1,
                _ => 2,
            };
            let [x, y, z] = [sides[turn], sides[(turn + 1) % 3], sides[(turn + 2) % 3]];
            if !long[(turn + 1) % 3] {
                let m = median(x, y, z);
                return pieces([x / 2.0, m, z], limit, depth + 1) + pieces([x / 2.0, y, m], limit, depth + 1);
            }
            // The corner between the two long sides comes off; the rest is cut along its
            // shorter diagonal.
            let corner = pieces([x / 2.0, y / 2.0, z / 2.0], limit, depth + 1);
            let (to_c, to_a) = (median(x, y, z), median(y, z, x));
            let rest = if to_c <= to_a {
                pieces([x / 2.0, to_c, z], limit, depth + 1) + pieces([z / 2.0, y / 2.0, to_c], limit, depth + 1)
            } else {
                pieces([x / 2.0, z / 2.0, to_a], limit, depth + 1) + pieces([to_a, y / 2.0, z], limit, depth + 1)
            };
            corner + rest
        }
    }
}

/// The edge length, `wanted` or longer, with which [`subdivide`] stays within about `budget`
/// triangles.
pub fn edge_for_budget(mesh: &Mesh, wanted: f32, budget: usize, refine: Option<&[bool]>) -> f32 {
    let mut edge = wanted.max(1e-6);
    let budget = budget.max(1);
    for _ in 0..32 {
        let estimated = estimate(mesh, edge, refine, budget.saturating_mul(8));
        if estimated <= budget {
            break;
        }
        // The count goes with 1 / edge², though in steps: halving happens or it does not. So
        // grow by a little more than the count asks for, and at least by a noticeable step.
        edge *= ((estimated as f32 / budget as f32).sqrt() * 1.05).clamp(1.05, 3.0);
    }
    edge
}

#[inline]
fn edge_key(a: u32, b: u32) -> u64 {
    (u64::from(a.min(b)) << 32) | u64::from(a.max(b))
}

#[inline]
fn distance_squared(mesh: &Mesh, a: u32, b: u32) -> f64 {
    let d = vec::sub64(vec::to64(mesh.positions[a as usize]), vec::to64(mesh.positions[b as usize]));
    vec::dot64(d, d)
}

fn has_long_edge(out: &Subdivided, limit_squared: f64, refined: &impl Fn(u32) -> bool) -> bool {
    out.mesh.triangles.iter().enumerate().any(|(t, corners)| {
        refined(out.parent[t])
            && (0..3).any(|k| distance_squared(&out.mesh, corners[k], corners[(k + 1) % 3]) > limit_squared)
    })
}

/// Cuts one triangle along the middles of its sides that have one, keeping its facing.
fn split(mesh: &Mesh, corners: [u32; 3], middle: [Option<u32>; 3], into: &mut Vec<[u32; 3]>) {
    let [a, b, c] = corners;
    match middle {
        [None, None, None] => into.push(corners),
        [Some(ab), Some(bc), Some(ca)] => {
            into.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
        }
        _ => {
            // Turn the triangle so the cases are: one middle on the first side, or middles on the
            // first and second side.
            let turn = match middle {
                [Some(_), None, None] | [Some(_), Some(_), None] => 0,
                [None, Some(_), None] | [None, Some(_), Some(_)] => 1,
                _ => 2,
            };
            let [a, b, c] = [corners[turn], corners[(turn + 1) % 3], corners[(turn + 2) % 3]];
            let ab = middle[turn].expect("the turn puts a middle on the first side");
            match middle[(turn + 1) % 3] {
                None => into.extend([[a, ab, c], [ab, b, c]]),
                Some(bc) => {
                    // The corner at b comes off; the rest is a four-sided piece a–ab–bc–c, cut
                    // along its shorter diagonal.
                    into.push([ab, b, bc]);
                    if distance_squared(mesh, ab, c) <= distance_squared(mesh, a, bc) {
                        into.extend([[a, ab, c], [ab, bc, c]]);
                    } else {
                        into.extend([[a, ab, bc], [a, bc, c]]);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::diagnose;
    use crate::shapes;

    fn longest_edge(mesh: &Mesh) -> f64 {
        mesh.triangles
            .iter()
            .flat_map(|t| (0..3).map(move |k| (t[k], t[(k + 1) % 3])))
            .map(|(a, b)| distance_squared(mesh, a, b).sqrt())
            .fold(0.0, f64::max)
    }

    fn all(max_edge: f32) -> Limits<'static> {
        Limits { max_edge, max_triangles: usize::MAX, refine: None }
    }

    #[test]
    fn a_cube_stays_a_cube_with_short_edges() {
        let cube = shapes::cuboid([10.0, 20.0, 30.0]);
        let fine = subdivide(&cube, all(1.0));
        assert!(longest_edge(&fine.mesh) <= 1.0);
        let report = diagnose(&fine.mesh);
        assert!(report.sound(), "{report:?}");
        assert_eq!((report.shells, report.euler(), report.flat_triangles), (1, 2, 0));
        assert!((report.volume - 6000.0).abs() < 1e-2 && (report.area - 2200.0).abs() < 1e-2);
        assert!(!fine.capped && fine.passes >= 5);
        // Every piece lies in the plane of the triangle it came from.
        let normals = crate::normals::face_normals(&cube);
        for (t, &parent) in fine.parent.iter().enumerate() {
            assert_eq!(crate::normals::face_normal(&fine.mesh, t), normals[parent as usize]);
        }
        // Cut in thought it is the same number.
        assert_eq!(estimate(&cube, 1.0, None, usize::MAX), fine.mesh.triangles.len());
        assert!(estimate(&cube, 1.0, None, 500) <= 500 + fine.mesh.triangles.len() / 12);
    }

    #[test]
    fn round_and_open_shapes_keep_their_kind() {
        let coarse = shapes::torus(20.0, 5.0, 24, 12);
        let ring = subdivide(&coarse, all(0.8));
        assert_eq!(estimate(&coarse, 0.8, None, usize::MAX), ring.mesh.triangles.len());
        // Long thin triangles, as CAD programs make them for round walls.
        let thin = shapes::sheet([180.0, 1.3], [1, 1]);
        assert_eq!(estimate(&thin, 0.5, None, usize::MAX), subdivide(&thin, all(0.5)).mesh.triangles.len());
        let report = diagnose(&ring.mesh);
        assert!(report.sound() && report.euler() == 0 && longest_edge(&ring.mesh) <= 0.8);

        let sheet = shapes::sheet([30.0, 20.0], [3, 2]);
        let fine = subdivide(&sheet, all(0.7));
        let report = diagnose(&fine.mesh);
        assert_eq!((report.non_manifold_edges, report.disagreeing_edges, report.shells), (0, 0, 1));
        assert!((report.area - 600.0).abs() < 1e-3);
        // The rim is still one closed line: as many rim edges as rim vertices.
        assert_eq!(report.euler(), 1);
    }

    #[test]
    fn only_the_chosen_triangles_are_refined_and_no_gap_opens() {
        let cube = shapes::cuboid([10.0; 3]);
        let mut refine = vec![false; 12];
        refine[2] = true;
        refine[3] = true; // the top
        let fine = subdivide(&cube, Limits { max_edge: 1.0, max_triangles: usize::MAX, refine: Some(&refine) });
        let report = diagnose(&fine.mesh);
        assert!(report.sound(), "{report:?}");
        assert!((report.volume - 1000.0).abs() < 1e-3);
        let pieces = |parent: u32| fine.parent.iter().filter(|&&p| p == parent).count();
        assert_eq!(pieces(0), 1, "the bottom is untouched");
        // Triangle 5 is the half of the front that reaches up to the top: cut only along that edge.
        assert_eq!(pieces(4), 1);
        assert!(pieces(2) > 100 && pieces(5) > 1 && pieces(5) < pieces(2) / 4);
        // Within the top no long edge is left.
        for (t, corners) in fine.mesh.triangles.iter().enumerate() {
            if fine.parent[t] == 2 || fine.parent[t] == 3 {
                assert!((0..3).all(|k| distance_squared(&fine.mesh, corners[k], corners[(k + 1) % 3]) <= 1.0));
            }
        }
    }

    #[test]
    fn a_budget_stops_the_cutting_and_says_so() {
        let cube = shapes::cuboid([100.0; 3]);
        let fine = subdivide(&cube, Limits { max_edge: 0.5, max_triangles: 20_000, refine: None });
        assert!(fine.capped);
        assert!(fine.mesh.triangles.len() >= 20_000 && fine.mesh.triangles.len() < 100_000);
        assert!(diagnose(&fine.mesh).sound());

        let edge = edge_for_budget(&cube, 0.5, 20_000, None);
        assert!(edge > 0.5);
        let fitted = subdivide(&cube, all(edge));
        assert!(!fitted.capped && fitted.mesh.triangles.len() <= 20_000, "{}", fitted.mesh.triangles.len());
        assert_eq!(edge_for_budget(&cube, 50.0, 20_000, None), 50.0);
    }

    #[test]
    fn a_fin_is_cut_for_all_three_of_its_triangles() {
        let mut cube = shapes::cuboid([4.0; 3]);
        cube.positions.push([2.0, -4.0, -4.0]);
        cube.triangles.push([0, 1, 8]);
        let before = diagnose(&cube);
        let fine = subdivide(&cube, all(1.0));
        let after = diagnose(&fine.mesh);
        // Four pieces of the edge where three triangles met, each still with three; the two rims
        // of the fin got longer, nothing else opened.
        assert_eq!(before.non_manifold_edges, 1);
        assert_eq!(after.non_manifold_edges, 4);
        assert_eq!(after.shells, 1);
        assert!(after.open_edges > before.open_edges);
    }
}
