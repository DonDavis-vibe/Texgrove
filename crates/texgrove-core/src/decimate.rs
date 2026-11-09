//! Making a mesh smaller: fewer triangles, as near to the same shape as they can be.
//!
//! Refining for a texture leaves millions of triangles, most of them where nothing happens.
//! Here edges are pulled together to a point, one after another, always the one that changes
//! the shape least (Garland and Heckbert's quadric error metric): every vertex remembers the
//! planes of the triangles it stood for, and an edge costs what its new point lies off them.
//!
//! What must not happen is refused: a rim keeps its line, surfaces do not grow together,
//! triangles do not turn over, and where three or more triangles share an edge nothing moves.

use crate::mesh::Mesh;
use crate::topology::{EdgeKind, Topology};
use crate::vec::{self, D3};
use std::collections::VecDeque;

/// How far to go.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    /// Stop once the mesh has no more triangles than this.
    pub triangles: usize,
    /// Below the budget, still take away what moves the surface by less than this (mm): flat
    /// stretches cut fine for nothing. 0 stops at the budget.
    pub tolerance: f32,
}

/// A smaller mesh, and where its triangles came from.
#[derive(Debug, Clone, Default)]
pub struct Decimated {
    pub mesh: Mesh,
    /// For every triangle: the triangle of the given mesh it is what is left of.
    pub source_triangle: Vec<u32>,
    /// Whether the work was stopped from outside before it reached its target.
    pub stopped: bool,
}

/// Mark of a vertex on a rim.
const RIM: u8 = 1;
/// Mark of a vertex that must stay as it is: on an edge with three or more triangles.
const FIXED: u8 = 2;
const NONE: u32 = u32::MAX;
/// A rim's line counts this many times as much as a face of the same size.
const RIM_WEIGHT: f64 = 1000.0;
/// Triangles may turn by this much (as the cosine of the angle) when a vertex moves, not more.
const TURN_LIMIT: f64 = 0.2;

/// The planes a vertex stands for, as the ten different numbers of a symmetric 4 × 4 matrix:
/// `p · Q · p` is the summed squared distance of `p` from them, each weighted by its area.
#[derive(Debug, Clone, Copy, Default)]
struct Quadric([f64; 10]);

impl Quadric {
    fn plane(normal: D3, through: D3, weight: f64) -> Quadric {
        let [a, b, c] = normal;
        let d = -vec::dot64(normal, through);
        Quadric([a * a, a * b, a * c, a * d, b * b, b * c, b * d, c * c, c * d, d * d].map(|v| v * weight))
    }

    fn add(&mut self, other: &Quadric) {
        for (mine, theirs) in self.0.iter_mut().zip(&other.0) {
            *mine += theirs;
        }
    }

    fn sum(a: &Quadric, b: &Quadric) -> Quadric {
        let mut q = *a;
        q.add(b);
        q
    }

    fn cost(&self, [x, y, z]: D3) -> f64 {
        let q = &self.0;
        let cost = q[0] * x * x
            + 2.0 * q[1] * x * y
            + 2.0 * q[2] * x * z
            + 2.0 * q[3] * x
            + q[4] * y * y
            + 2.0 * q[5] * y * z
            + 2.0 * q[6] * y
            + q[7] * z * z
            + 2.0 * q[8] * z
            + q[9];
        cost.max(0.0)
    }

    /// The point that lies nearest to all the planes, if they pin one down.
    fn best(&self) -> Option<D3> {
        let q = &self.0;
        // Solve the 3 × 3 system A · p = −b by Cramer's rule.
        let (a, b, c, d, e, f) = (q[0], q[1], q[2], q[4], q[5], q[7]);
        let det = a * (d * f - e * e) - b * (b * f - e * c) + c * (b * e - d * c);
        let size = (a + d + f) / 3.0;
        let pinned = det.is_finite() && det.abs() > 1e-9 * size * size * size;
        if !pinned {
            return None; // planes all (nearly) parallel or along one line: any point on it is as good
        }
        let (r0, r1, r2) = (-q[3], -q[6], -q[8]);
        let x = r0 * (d * f - e * e) - b * (r1 * f - e * r2) + c * (r1 * e - d * r2);
        let y = a * (r1 * f - e * r2) - r0 * (b * f - e * c) + c * (b * r2 - r1 * c);
        let z = a * (d * r2 - r1 * e) - b * (b * r2 - r1 * c) + r0 * (b * e - d * c);
        Some([x / det, y / det, z / det])
    }
}

/// An edge waiting to be pulled together.
#[derive(Debug, Clone, Copy)]
struct Candidate {
    a: u32,
    b: u32,
    /// The sum of both vertices' change counts when the cost was worked out: if it differs now,
    /// the edge has changed and the candidate is out of date.
    stamp: u32,
}

/// The waiting edges, cheapest first, sorted roughly: into drawers by the size of their cost
/// (eight drawers for every doubling), and within a drawer the oldest first.
///
/// A heap would sort exactly, but with tens of millions of entries every look into it misses
/// the processor's cache several times over, and that was most of the time reducing took. Two
/// edges within nine percent of each other's cost may now go in either order, which no eye
/// can tell; the order is still the same on every run.
struct Queue {
    drawers: Vec<VecDeque<Candidate>>,
    /// One bit per drawer that has something in it, to find the next one without looking
    /// through hundreds of empty ones.
    filled: [u64; Queue::DRAWERS / 64],
}

impl Queue {
    /// Eight drawers for each doubling of the cost, from 2⁻⁸⁰ (far below anything a mesh in
    /// millimetres can mean; all that is smaller, or nothing at all, shares the first drawer)
    /// up to 2⁴⁸.
    const DRAWERS: usize = 128 * 8;
    /// The drawer number an `f32` of 2⁻⁸⁰ would have if they were numbered from the smallest float.
    const FIRST: usize = (127 - 80) * 8;

    fn new() -> Queue {
        Queue { drawers: vec![VecDeque::new(); Queue::DRAWERS], filled: [0; Queue::DRAWERS / 64] }
    }

    fn push(&mut self, cost: f64, candidate: Candidate) {
        // The bits of a positive float rise with its value: the exponent and the first three
        // bits after it number the drawer.
        let cost = if cost.is_finite() { (cost as f32).max(0.0) } else { f32::MAX };
        let drawer = ((cost.to_bits() >> 20) as usize).saturating_sub(Queue::FIRST).min(Queue::DRAWERS - 1);
        self.drawers[drawer].push_back(candidate);
        self.filled[drawer / 64] |= 1 << (drawer % 64);
    }

    fn pop(&mut self) -> Option<Candidate> {
        let (word, bits) = self.filled.iter().enumerate().find(|(_, bits)| **bits != 0)?;
        let drawer = word * 64 + bits.trailing_zeros() as usize;
        // The oldest first. Taking the newest would follow each new point straight to its next
        // edge, and one point would swallow a whole flat face: a fan of thousands of slivers,
        // each step slower than the last.
        let candidate = self.drawers[drawer].pop_front();
        if self.drawers[drawer].is_empty() {
            self.filled[word] &= !(1 << (drawer % 64));
            // A large drawer gives its memory back once it is empty: the costs move upwards as
            // the work goes on, and it will not fill like that again.
            if self.drawers[drawer].capacity() > 1 << 16 {
                self.drawers[drawer] = VecDeque::new();
            }
        }
        candidate
    }
}

struct Work {
    positions: Vec<D3>,
    triangles: Vec<[u32; 3]>,
    /// Triangles taken away.
    gone: Vec<bool>,
    quadrics: Vec<Quadric>,
    /// The area each vertex stands for, to turn a cost into a distance.
    areas: Vec<f32>,
    marks: Vec<u8>,
    stamps: Vec<u32>,
    /// The corners of each vertex as a chain: the first, and for every corner (triangle × 3 +
    /// place) the next one of the same vertex.
    first_corner: Vec<u32>,
    next_corner: Vec<u32>,
    /// Scratch: a tick per vertex, to tell "seen in this step" without clearing.
    seen: Vec<u32>,
    tick: u32,
    left: usize,
    /// Lists lent out and taken back, so the inner loop does not ask for memory.
    spare: [Vec<u32>; 3],
}

/// Reduces a welded mesh ([`crate::weld`]) towards the target. `go_on` is asked now and then with
/// the part of the way done (0 to 1) and stops the work by answering `false`; what was reached
/// by then is returned as a whole mesh.
pub fn decimate(mesh: &Mesh, target: Target, mut go_on: impl FnMut(f32) -> bool) -> Decimated {
    let start = mesh.triangles.len();
    if start <= target.triangles && target.tolerance <= 0.0 {
        return Decimated { mesh: mesh.clone(), source_triangle: (0..start as u32).collect(), stopped: false };
    }
    let mut work = Work::new(mesh);
    let mut queue = Queue::new();
    let mut around = Vec::new();
    for v in 0..work.positions.len() as u32 {
        work.neighbours(v, &mut around);
        for &n in around.iter().filter(|&&n| n > v) {
            if let Some((cost, _)) = work.plan(v, n) {
                queue.push(work.rank(v, n, cost), Candidate { a: v, b: n, stamp: work.stamp(v, n) });
            }
        }
    }
    let tolerance = f64::from(target.tolerance.max(0.0));
    let mut stopped = false;
    let mut steps = 0u32;
    while let Some(candidate) = queue.pop() {
        let (a, b) = (candidate.a, candidate.b);
        if candidate.stamp != work.stamp(a, b) || work.first_corner[a as usize] == NONE {
            continue; // out of date
        }
        let Some((cost, place)) = work.plan(a, b) else { continue };
        if work.left <= target.triangles {
            // Under the budget only what is next to free goes on: the cost is a summed squared
            // distance weighted by area, so divided by the area it is a distance squared.
            let area = f64::from(work.areas[a as usize] + work.areas[b as usize]);
            if tolerance <= 0.0 || area <= 0.0 || cost / area > tolerance * tolerance {
                break;
            }
        }
        steps += 1;
        if steps.is_multiple_of(1 << 16) {
            let whole = start.saturating_sub(target.triangles).max(1) as f32;
            if !go_on(((start - work.left) as f32 / whole).min(1.0)) {
                stopped = true;
                break;
            }
        }
        if !work.pull_together(a, b, place) {
            continue;
        }
        // `b` is what is left of both: its edges cost something new now.
        work.neighbours(b, &mut around);
        for &n in &around {
            if let Some((cost, _)) = work.plan(b, n) {
                queue.push(work.rank(b, n, cost), Candidate { a: b.min(n), b: b.max(n), stamp: work.stamp(b, n) });
            }
        }
    }
    work.finish(stopped)
}

impl Work {
    fn new(mesh: &Mesh) -> Work {
        let vertices = mesh.positions.len();
        let mut work = Work {
            positions: mesh.positions.iter().map(|&p| vec::to64(p)).collect(),
            triangles: mesh.triangles.clone(),
            gone: vec![false; mesh.triangles.len()],
            quadrics: vec![Quadric::default(); vertices],
            areas: vec![0.0; vertices],
            marks: vec![0; vertices],
            stamps: vec![0; vertices],
            first_corner: vec![NONE; vertices],
            next_corner: vec![NONE; mesh.triangles.len() * 3],
            seen: vec![0; vertices],
            tick: 0,
            left: mesh.triangles.len(),
            spare: Default::default(),
        };
        for (t, corners) in mesh.triangles.iter().enumerate() {
            let twice_area = mesh.area_vector(t);
            let len = vec::length64(twice_area);
            for (k, &v) in corners.iter().enumerate() {
                let corner = (t * 3 + k) as u32;
                work.next_corner[corner as usize] = work.first_corner[v as usize];
                work.first_corner[v as usize] = corner;
            }
            if len <= 0.0 {
                continue;
            }
            let normal = twice_area.map(|c| c / len);
            let plane = Quadric::plane(normal, work.positions[corners[0] as usize], len / 2.0);
            for &v in corners {
                work.quadrics[v as usize].add(&plane);
                work.areas[v as usize] += (len / 2.0) as f32;
            }
        }
        // Rims get a wall along them, so their line is as dear as a face; fins are not touched.
        let topology = Topology::build(mesh);
        for edge in 0..topology.edge_count() as u32 {
            match topology.kind(edge) {
                EdgeKind::Manifold => {}
                EdgeKind::NonManifold => {
                    for v in topology.edge_vertices(mesh, edge) {
                        work.marks[v as usize] |= FIXED;
                    }
                }
                EdgeKind::Boundary => {
                    let side = topology.edge_sides(edge).next().expect("an edge has a side");
                    let [from, to] = side.vertices(mesh);
                    let along = vec::sub64(work.positions[to as usize], work.positions[from as usize]);
                    let across = vec::cross64(along, mesh.area_vector(side.triangle as usize));
                    let len = vec::length64(across);
                    for v in [from, to] {
                        work.marks[v as usize] |= RIM;
                        if len > 0.0 {
                            let wall = Quadric::plane(
                                across.map(|c| c / len),
                                work.positions[from as usize],
                                RIM_WEIGHT * vec::dot64(along, along),
                            );
                            work.quadrics[v as usize].add(&wall);
                        }
                    }
                }
            }
        }
        work
    }

    #[inline]
    fn stamp(&self, a: u32, b: u32) -> u32 {
        self.stamps[a as usize].wrapping_add(self.stamps[b as usize])
    }

    /// What an edge is sorted by: its cost, and a trace of its length, so that where the cost is
    /// nothing (a flat face) short edges go first and the face thins out evenly.
    #[inline]
    fn rank(&self, a: u32, b: u32, cost: f64) -> f64 {
        let along = vec::sub64(self.positions[a as usize], self.positions[b as usize]);
        cost + 1e-9 * vec::dot64(along, along)
    }

    /// The corners of a vertex that still belong to a triangle; those that do not are taken out
    /// of the chain on the way.
    fn corners(&mut self, v: u32, into: &mut Vec<u32>) {
        into.clear();
        let mut previous = NONE;
        let mut corner = self.first_corner[v as usize];
        while corner != NONE {
            let next = self.next_corner[corner as usize];
            if self.gone[corner as usize / 3] {
                if previous == NONE {
                    self.first_corner[v as usize] = next;
                } else {
                    self.next_corner[previous as usize] = next;
                }
            } else {
                into.push(corner);
                previous = corner;
            }
            corner = next;
        }
    }

    /// The vertices joined to `v` by an edge, each once, in rising order.
    fn neighbours(&mut self, v: u32, into: &mut Vec<u32>) {
        let mut corners = std::mem::take(&mut self.spare[0]);
        self.corners(v, &mut corners);
        into.clear();
        for &corner in &corners {
            let triangle = self.triangles[corner as usize / 3];
            into.extend(triangle.iter().filter(|&&other| other != v));
        }
        into.sort_unstable();
        into.dedup();
        self.spare[0] = corners;
    }

    /// What pulling an edge together would cost and where the new point would be; `None` if the
    /// edge is not to be touched. The vertex that is to stay is the second one, `b`, unless its
    /// marks say otherwise: then the place is the marked vertex's own.
    fn plan(&self, a: u32, b: u32) -> Option<(f64, D3)> {
        let (pa, pb) = (self.positions[a as usize], self.positions[b as usize]);
        let (ma, mb) = (self.marks[a as usize], self.marks[b as usize]);
        if ma & FIXED != 0 && mb & FIXED != 0 {
            return None;
        }
        let quadric = Quadric::sum(&self.quadrics[a as usize], &self.quadrics[b as usize]);
        // A vertex that must stay, or one on a rim when the other is not, keeps its place.
        let held = if ma & FIXED != 0 || (ma & RIM != 0 && mb & (RIM | FIXED) == 0) {
            Some(pa)
        } else if mb & FIXED != 0 || (mb & RIM != 0 && ma & (RIM | FIXED) == 0) {
            Some(pb)
        } else {
            None
        };
        if let Some(place) = held {
            return Some((quadric.cost(place), place));
        }
        let middle = [0, 1, 2].map(|i| (pa[i] + pb[i]) / 2.0);
        let length_squared = vec::dot64(vec::sub64(pa, pb), vec::sub64(pa, pb));
        // The best point by the planes, unless they send it far off (when they are nearly
        // parallel, a hair of noise decides): then the better of the ends and the middle.
        let best = quadric.best().filter(|p| {
            let off = vec::sub64(*p, middle);
            vec::dot64(off, off) <= 4.0 * length_squared && p.iter().all(|c| c.is_finite())
        });
        let mut place = best.unwrap_or_else(|| {
            [pa, pb, middle].into_iter().min_by(|x, y| quadric.cost(*x).total_cmp(&quadric.cost(*y))).unwrap_or(middle)
        });
        // A corner worked out from its planes comes back with the last digits off; where the
        // point is an end of the edge but for that, it is that end, exactly.
        let reach = pa.iter().chain(&pb).fold(1.0f64, |m, c| m.max(c.abs()));
        for end in [pa, pb] {
            let off = vec::sub64(place, end);
            if vec::dot64(off, off) <= (1e-9 * reach).powi(2) {
                place = end;
            }
        }
        Some((quadric.cost(place), place))
    }

    /// Pulls the edge `a`–`b` together to `place`: `a` goes, `b` stays there. Refuses (and
    /// changes nothing) when that would harm the mesh.
    fn pull_together(&mut self, a: u32, b: u32, place: D3) -> bool {
        let (mut corners_a, mut corners_b) = (std::mem::take(&mut self.spare[1]), std::mem::take(&mut self.spare[2]));
        self.corners(a, &mut corners_a);
        self.corners(b, &mut corners_b);
        let done = self.pull_together_with(a, b, place, &corners_a, &corners_b);
        self.spare[1] = corners_a;
        self.spare[2] = corners_b;
        done
    }

    fn pull_together_with(&mut self, a: u32, b: u32, place: D3, corners_a: &[u32], corners_b: &[u32]) -> bool {
        // The triangles on the edge go; there must be two of them, or one on a rim.
        let shared = corners_a.iter().filter(|&&c| self.triangles[c as usize / 3].contains(&b)).count();
        let both_on_rim = self.marks[a as usize] & RIM != 0 && self.marks[b as usize] & RIM != 0;
        // A triangle that goes must not be one that hangs on an edge that is to stay as it is.
        let fixed = |v: u32| self.marks[v as usize] & FIXED != 0;
        if corners_a
            .iter()
            .map(|&c| self.triangles[c as usize / 3])
            .any(|t| t.contains(&b) && t.iter().filter(|&&v| fixed(v)).count() >= 2)
        {
            return false;
        }
        match shared {
            1 if both_on_rim => {}
            // Two rim vertices with a whole edge between them: pulling it together would pinch
            // the surface off.
            2 if !both_on_rim => {}
            _ => return false,
        }
        // The vertices joined to both ends must be exactly the third corners of the triangles on
        // the edge; any other would end up with two surfaces glued along a line.
        self.tick += 2;
        let (near_a, counted) = (self.tick - 1, self.tick);
        for &corner in corners_a {
            for other in self.triangles[corner as usize / 3] {
                self.seen[other as usize] = near_a;
            }
        }
        let mut common = 0;
        for &corner in corners_b {
            for other in self.triangles[corner as usize / 3] {
                if other != a && other != b && self.seen[other as usize] == near_a {
                    self.seen[other as usize] = counted;
                    common += 1;
                }
            }
        }
        if common != shared {
            return false;
        }
        // No triangle that stays may turn over or lose its area.
        for (&corner, moved) in corners_a.iter().map(|c| (c, a)).chain(corners_b.iter().map(|c| (c, b))) {
            let triangle = self.triangles[corner as usize / 3];
            if triangle.contains(&a) && triangle.contains(&b) {
                continue;
            }
            let before = triangle.map(|v| self.positions[v as usize]);
            let after = triangle.map(|v| if v == moved { place } else { self.positions[v as usize] });
            let normal = |p: [D3; 3]| vec::cross64(vec::sub64(p[1], p[0]), vec::sub64(p[2], p[0]));
            let (old, new) = (normal(before), normal(after));
            let (old_len, new_len) = (vec::length64(old), vec::length64(new));
            if old_len > 0.0 && (new_len <= 1e-12 * old_len || vec::dot64(old, new) < TURN_LIMIT * old_len * new_len) {
                return false;
            }
        }
        // Do it.
        for &corner in corners_a {
            let t = corner as usize / 3;
            if self.triangles[t].contains(&b) {
                self.gone[t] = true;
                self.left -= 1;
            } else {
                self.triangles[t][corner as usize % 3] = b;
            }
        }
        // The chain of `a` goes in front of that of `b`.
        if let Some(&last) = corners_a.last() {
            // `corners` left only living corners in the chain, so `last` is its end.
            self.next_corner[last as usize] = self.first_corner[b as usize];
            self.first_corner[b as usize] = self.first_corner[a as usize];
        }
        self.first_corner[a as usize] = NONE;
        self.positions[b as usize] = place;
        let quadric = self.quadrics[a as usize];
        self.quadrics[b as usize].add(&quadric);
        self.areas[b as usize] += self.areas[a as usize];
        self.marks[b as usize] |= self.marks[a as usize];
        self.stamps[a as usize] = self.stamps[a as usize].wrapping_add(1);
        self.stamps[b as usize] = self.stamps[b as usize].wrapping_add(1);
        true
    }

    /// The triangles that are left, with only the vertices they use.
    fn finish(self, stopped: bool) -> Decimated {
        let mut number = vec![NONE; self.positions.len()];
        let mut out = Decimated { stopped, ..Decimated::default() };
        out.mesh.triangles.reserve(self.left);
        out.source_triangle.reserve(self.left);
        for (t, triangle) in self.triangles.iter().enumerate() {
            if self.gone[t] {
                continue;
            }
            let corners = triangle.map(|v| {
                if number[v as usize] == NONE {
                    number[v as usize] = out.mesh.positions.len() as u32;
                    out.mesh.positions.push(self.positions[v as usize].map(|c| c as f32));
                }
                number[v as usize]
            });
            out.mesh.triangles.push(corners);
            out.source_triangle.push(t as u32);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::diagnose;
    use crate::pattern::Pattern;
    use crate::pipeline::{self, Finish, Settings};
    use crate::project::Placement;
    use crate::shapes;
    use crate::subdivide::{Limits, subdivide};

    fn to(triangles: usize) -> Target {
        Target { triangles, tolerance: 0.0 }
    }

    fn fine(mesh: &Mesh, max_edge: f32) -> Mesh {
        subdivide(mesh, Limits { max_edge, max_triangles: usize::MAX, refine: None }).mesh
    }

    #[test]
    fn a_ball_gets_smaller_and_stays_a_ball() {
        let ball = shapes::sphere(10.0, 96, 48);
        let small = decimate(&ball, to(2000), |_| true);
        let report = diagnose(&small.mesh);
        assert!(report.sound(), "{report:?}");
        assert_eq!((report.shells, report.euler()), (1, 2));
        assert!(report.triangles <= 2000 && report.triangles > 1900, "{}", report.triangles);
        for p in &small.mesh.positions {
            assert!((vec::length(*p) - 10.0).abs() < 0.08, "{p:?}");
        }
        assert!((report.volume - ball.volume()).abs() < ball.volume() * 0.01);
        // What is left of a triangle still faces about the way it did.
        let normals = crate::normals::face_normals(&ball);
        for (t, &source) in small.source_triangle.iter().enumerate() {
            let now = crate::normals::face_normal(&small.mesh, t);
            assert!(vec::dot(now, normals[source as usize]) > 0.8);
        }
    }

    #[test]
    fn flat_faces_cut_fine_for_nothing_cost_nothing_to_take_back() {
        let cube = fine(&shapes::cuboid([20.0, 30.0, 40.0]), 1.0);
        assert!(cube.triangles.len() > 10_000);
        // No budget at all: only what moves the surface by less than a thousandth of a mm goes.
        let back = decimate(&cube, Target { triangles: usize::MAX, tolerance: 0.001 }, |_| true);
        let report = diagnose(&back.mesh);
        assert!(report.sound(), "{report:?}");
        assert!(report.triangles < 200, "{}", report.triangles);
        assert!((report.volume - 24_000.0).abs() < 1e-6 && (report.area - 5200.0).abs() < 1e-6);
        assert_eq!(report.bounds.size(), [20.0, 30.0, 40.0]);
    }

    #[test]
    fn a_rim_keeps_its_line() {
        let sheet = fine(&shapes::sheet([30.0, 20.0], [3, 2]), 0.8);
        let small = decimate(&sheet, to(60), |_| true);
        let report = diagnose(&small.mesh);
        assert!(report.triangles <= 60);
        assert_eq!((report.non_manifold_edges, report.disagreeing_edges, report.shells, report.euler()), (0, 0, 1, 1));
        assert!((report.area - 600.0).abs() < 1e-6, "{}", report.area);
        assert_eq!((report.bounds.min, report.bounds.max), ([0.0; 3], [30.0, 20.0, 0.0]));
    }

    #[test]
    fn a_ring_keeps_its_hole_and_a_fin_stays() {
        let ring = shapes::torus(20.0, 6.0, 96, 48);
        let small = decimate(&ring, to(1500), |_| true);
        let report = diagnose(&small.mesh);
        assert!(report.sound() && report.euler() == 0 && report.triangles <= 1500, "{report:?}");

        let mut finned = fine(&shapes::cuboid([4.0; 3]), 1.0);
        let extra = finned.positions.len() as u32;
        finned.positions.push([2.0, -4.0, -4.0]);
        let (from, to_vertex) = {
            // Two neighbouring vertices on the cube's edge along x at y = z = 0.
            let on = |x: f32| finned.positions.iter().position(|p| *p == [x, 0.0, 0.0]).unwrap() as u32;
            (on(1.0), on(2.0))
        };
        finned.triangles.push([from, to_vertex, extra]);
        let before = diagnose(&finned);
        assert_eq!(before.non_manifold_edges, 1);
        let small = decimate(&finned, to(40), |_| true);
        let after = diagnose(&small.mesh);
        assert_eq!(after.non_manifold_edges, 1);
        assert!(small.mesh.positions.contains(&[1.0, 0.0, 0.0]) && small.mesh.positions.contains(&[2.0, 0.0, 0.0]));
        assert!(after.triangles < before.triangles / 4);
    }

    #[test]
    fn a_texture_survives_a_fifth_of_its_triangles() {
        let cube = shapes::cuboid([20.0; 3]);
        let settings = Settings {
            placement: Placement { tile: 5.0, ..Placement::default() },
            finish: Finish { max_edge: 0.3, triangles: usize::MAX, tolerance: 0.0, ..Finish::default() },
            ..Settings::default()
        };
        let textured = pipeline::texture(&cube, &Pattern::Knurl.render(256), &settings).mesh;
        let before = diagnose(&textured);
        let small = decimate(&textured, to(before.triangles / 5), |_| true);
        let after = diagnose(&small.mesh);
        assert!(after.sound(), "{after:?}");
        assert!(after.triangles <= before.triangles / 5);
        assert!((after.volume - before.volume).abs() < before.volume * 0.002, "{} {}", after.volume, before.volume);
        assert!((after.area - before.area).abs() < before.area * 0.02, "{} {}", after.area, before.area);
        let (a, b) = (after.bounds.size(), before.bounds.size());
        assert!((0..3).all(|i| (a[i] - b[i]).abs() < 0.02), "{a:?} {b:?}");
    }

    #[test]
    fn nothing_to_do_changes_nothing_and_stopping_leaves_a_whole_mesh() {
        let ball = shapes::sphere(10.0, 24, 12);
        let same = decimate(&ball, to(usize::MAX), |_| true);
        assert_eq!(same.mesh, ball);
        assert_eq!(same.source_triangle.len(), ball.triangles.len());

        let big = shapes::sphere(10.0, 640, 320);
        let mut asked = 0;
        let part = decimate(&big, to(100), |done| {
            asked += 1;
            assert!((0.0..=1.0).contains(&done));
            asked < 2
        });
        assert!(part.stopped && asked == 2);
        let report = diagnose(&part.mesh);
        assert!(report.sound() && report.triangles > 100 && report.triangles < big.triangles.len());
    }

    #[test]
    fn the_same_mesh_gives_the_same_result() {
        let ring = shapes::torus(20.0, 6.0, 64, 32);
        assert_eq!(decimate(&ring, to(800), |_| true).mesh, decimate(&ring, to(800), |_| true).mesh);
    }
}
