//! Where a texture goes and where it does not.
//!
//! A mask gives every vertex of the refined mesh a weight from 0 (stays where it is) to 1 (moves
//! all the way). Four things decide it: the direction a face looks in (leave out the bottom, the
//! top), faces picked one by one, strokes of a round brush, and a soft run-out towards everything
//! that stays.

use crate::bvh::Bvh;
use crate::mesh::Mesh;
use crate::normals::face_normal;
use crate::topology::VertexTriangles;
use crate::vec::{self, P3};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// A face picked to get no texture.
pub const EXCLUDED: u8 = 1;
/// A face picked to get texture; once any is picked, only picked faces (and brushed places) do.
pub const INCLUDED: u8 = 2;

/// What a brush stroke marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DabKind {
    /// No texture here.
    Exclude,
    /// Texture here; with any such dab, places not marked get none.
    Include,
}

/// One touch of the round brush.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dab {
    pub centre: P3,
    /// The surface's unit normal where the brush touched: only what faces the same way is
    /// marked, so a stroke does not go through a thin wall. Zero marks all within reach.
    pub normal: P3,
    /// The brush's radius in mm.
    pub radius: f32,
    /// How much of the radius marks fully, from 0 (soft all the way) to 1 (a hard rim).
    pub hardness: f32,
    pub kind: DabKind,
    /// Takes the mark away again instead of setting it.
    pub erase: bool,
}

/// Where a texture goes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mask {
    /// Faces looking up within this many degrees of straight up get no texture; 0 leaves the
    /// top in.
    pub top_degrees: f32,
    /// The same for faces looking down: what lies on the print bed.
    pub bottom_degrees: f32,
    /// Towards everything that stays in place the texture runs out over this many mm.
    pub falloff: f32,
    /// A mark per triangle of the model as loaded ([`EXCLUDED`], [`INCLUDED`] or 0); may be
    /// shorter than the model or empty.
    pub faces: Vec<u8>,
    /// Brush strokes, in the order they were made.
    pub dabs: Vec<Dab>,
    /// Texture only where it was asked for (picked faces, brushed places), even while nothing
    /// is: a layer that starts empty and is painted on.
    pub only_picked: bool,
}

/// Something the user did to say where a texture goes, kept as geometry: a point on the model,
/// not the number of a triangle. So it means the same on the coarse copy shown while working and
/// on the fine one the file is made from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    /// A click on a face: the whole smooth surface round it, up to edges sharper than `degrees`.
    /// Erasing takes every mark off that surface, also one it had before the user's strokes.
    Fill { point: P3, normal: P3, kind: DabKind, erase: bool, degrees: f32 },
    /// One touch of the brush.
    Dab(Dab),
    /// Everything before this is forgotten.
    Clear,
}

impl Mask {
    /// Works the user's strokes into the mask, in their order: fills become marks on the faces
    /// of `source` (the model as loaded, welded), dabs are kept as they are.
    ///
    /// A fill whose point is farther than `reach` mm from the model's surface is not meant for
    /// this model (a part of a project next to the one that was clicked) and is left out.
    pub fn apply_ops(&mut self, source: &Mesh, ops: &[Op], reach: f32) {
        // Only what follows the last clearing counts.
        let from = ops.iter().rposition(|op| matches!(op, Op::Clear)).map_or(0, |i| i + 1);
        if from > 0 {
            self.dabs.clear();
        }
        let ops = &ops[from..];
        let fills = ops.iter().any(|op| matches!(op, Op::Fill { .. }));
        let lookup = fills.then(|| (crate::topology::Topology::build(source), Bvh::build(source)));
        for op in ops {
            match *op {
                Op::Clear => {}
                Op::Dab(dab) => self.dabs.push(dab),
                Op::Fill { point, normal, kind, erase, degrees } => {
                    let Some((topology, bvh)) = &lookup else { continue };
                    let Some((nearest, _, distance)) = bvh.closest(source, point) else { continue };
                    if distance > reach {
                        continue;
                    }
                    // At an edge the nearest triangle may be the neighbouring face: of all that
                    // are about as near, the one facing the way the click did.
                    let mut near = Vec::new();
                    bvh.within(source, point, distance + reach.clamp(1e-4, 0.05), &mut near);
                    near.sort_unstable();
                    let facing = |t: u32| vec::dot(face_normal(source, t as usize), normal);
                    let start = near
                        .into_iter()
                        .fold(nearest, |best, t| if facing(t) > facing(best) + 1e-4 { t } else { best });
                    let mark = match (erase, kind) {
                        (true, _) => 0,
                        (false, DabKind::Exclude) => EXCLUDED,
                        (false, DabKind::Include) => INCLUDED,
                    };
                    if self.faces.len() < source.triangles.len() {
                        self.faces.resize(source.triangles.len(), 0);
                    }
                    for t in crate::select::surface(source, topology, start, degrees) {
                        self.faces[t as usize] = mark;
                    }
                }
            }
        }
    }

    /// Leaves out what looks down within `degrees`: the usual choice, so a part still lies flat.
    pub fn without_bottom(degrees: f32) -> Mask {
        Mask { bottom_degrees: degrees, ..Mask::default() }
    }

    fn mark(&self, triangle: usize) -> u8 {
        self.faces.get(triangle).copied().unwrap_or(0)
    }

    /// Whether texture only goes where it was asked for.
    fn picks(&self) -> bool {
        self.only_picked || self.faces.contains(&INCLUDED) || self.brushes_in()
    }

    fn brushes_in(&self) -> bool {
        self.dabs.iter().any(|dab| dab.kind == DabKind::Include && !dab.erase)
    }

    /// For every triangle of the model as loaded: whether any of it may get texture, and so is
    /// worth refining.
    pub fn active(&self, source: &Mesh) -> Vec<bool> {
        let (top, bottom) = (limit(self.top_degrees), limit(self.bottom_degrees));
        let picks = self.picks();
        let brushes_in = self.brushes_in();
        (0..source.triangles.len())
            .map(|t| {
                let up = face_normal(source, t)[2];
                if top.is_some_and(|limit| up >= limit) || bottom.is_some_and(|limit| -up >= limit) {
                    return false;
                }
                match self.mark(t) {
                    EXCLUDED => false,
                    INCLUDED => true,
                    // Not picked: only a brush can still bring texture here.
                    _ => !picks || brushes_in,
                }
            })
            .collect()
    }

    /// The weight of every vertex of the refined mesh.
    ///
    /// `active` is [`Mask::active`] for the model as loaded and `parent` says which of its
    /// triangles each refined one is a piece of. `held` marks vertices that stay in place for
    /// another reason (sharp edges); the run-out counts from them as well. `bvh` is a tree over
    /// the refined mesh, needed only when there are brush strokes.
    pub fn weights(
        &self,
        active: &[bool],
        refined: &Mesh,
        parent: &[u32],
        normals: &[P3],
        held: Option<&[bool]>,
        bvh: Option<&Bvh>,
    ) -> Vec<f32> {
        let mut weights = vec![1.0f32; refined.positions.len()];
        // A vertex next to a face that stays moves nothing: so that face stays flat to its rim.
        for (t, corners) in refined.triangles.iter().enumerate() {
            if !active[parent[t] as usize] {
                for &v in corners {
                    weights[v as usize] = 0.0;
                }
            }
        }
        if let Some(held) = held {
            for (weight, _) in weights.iter_mut().zip(held).filter(|(_, held)| **held) {
                *weight = 0.0;
            }
        }
        if self.picks() {
            // Picked faces count fully, and like faces that stay, a face not picked keeps its rim.
            let mut picked = vec![1.0f32; refined.positions.len()];
            for (t, corners) in refined.triangles.iter().enumerate() {
                if self.mark(parent[t] as usize) != INCLUDED {
                    for &v in corners {
                        picked[v as usize] = 0.0;
                    }
                }
            }
            self.brush(DabKind::Include, refined, normals, bvh, &mut picked);
            for (weight, picked) in weights.iter_mut().zip(&picked) {
                *weight *= picked;
            }
        }
        if self.dabs.iter().any(|dab| dab.kind == DabKind::Exclude) {
            let mut left_out = vec![0.0f32; refined.positions.len()];
            self.brush(DabKind::Exclude, refined, normals, bvh, &mut left_out);
            for (weight, left_out) in weights.iter_mut().zip(&left_out) {
                *weight *= 1.0 - left_out;
            }
        }
        if self.falloff > 0.0 {
            run_out(refined, &mut weights, self.falloff);
        }
        weights
    }

    /// Lays the dabs of one kind onto `marks` (0 to 1 per vertex), in their order.
    fn brush(&self, kind: DabKind, mesh: &Mesh, normals: &[P3], bvh: Option<&Bvh>, marks: &mut [f32]) {
        let mut near = Vec::new();
        for dab in self.dabs.iter().filter(|dab| dab.kind == kind && dab.radius > 0.0) {
            // The vertices in reach: through the tree if there is one, else all.
            near.clear();
            match bvh {
                Some(bvh) => bvh.within(mesh, dab.centre, dab.radius, &mut near),
                None => near.extend(0..mesh.triangles.len() as u32),
            }
            let full = dab.radius * dab.hardness.clamp(0.0, 1.0);
            for &t in &near {
                for v in mesh.triangles[t as usize] {
                    let v = v as usize;
                    if vec::dot(dab.normal, dab.normal) > 0.0 && vec::dot(normals[v], dab.normal) <= 0.0 {
                        continue;
                    }
                    let distance = vec::distance(mesh.positions[v], dab.centre);
                    let cover = if distance <= full {
                        1.0
                    } else if distance >= dab.radius {
                        0.0
                    } else {
                        1.0 - smooth((distance - full) / (dab.radius - full))
                    };
                    // A vertex is met once per triangle round it; setting a mark twice is the same
                    // as once, since both ways only take the larger or the smaller.
                    marks[v] = if dab.erase { marks[v].min(1.0 - cover) } else { marks[v].max(cover) };
                }
            }
        }
    }
}

/// The cosine a face's upward part must reach to count as within `degrees` of straight up; `None`
/// when the angle is 0 and nothing is left out.
fn limit(degrees: f32) -> Option<f32> {
    // A hair of slack, so "90 degrees" takes in walls that are upright but for rounding.
    (degrees > 0.0).then(|| libm::cosf(degrees.clamp(0.0, 180.0).to_radians()) - 1e-5)
}

#[inline]
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Scales the weights down towards every vertex with weight 0: nothing right next to it, all
/// from `reach` mm on, measured along the edges of the mesh.
fn run_out(mesh: &Mesh, weights: &mut [f32], reach: f32) {
    let round = VertexTriangles::build(mesh);
    let mut distance = vec![f32::INFINITY; mesh.positions.len()];
    let mut queue = BinaryHeap::new();
    for (v, _) in weights.iter().enumerate().filter(|(_, w)| **w <= 0.0) {
        distance[v] = 0.0;
        // Only those with a neighbour that moves have anything to hand on.
        let borders =
            round.of(v as u32).iter().any(|&t| mesh.triangles[t as usize].iter().any(|&n| weights[n as usize] > 0.0));
        if borders {
            queue.push((Reverse(0u32), v as u32));
        }
    }
    // Distances as whole micrometres: they sort without the trouble floats make in a heap.
    let micro = |d: f32| (d * 1000.0) as u32;
    while let Some((Reverse(at), v)) = queue.pop() {
        if at > micro(distance[v as usize]) {
            continue; // reached more shortly since
        }
        for &t in round.of(v) {
            for n in mesh.triangles[t as usize] {
                if n == v {
                    continue;
                }
                let through =
                    distance[v as usize] + vec::distance(mesh.positions[v as usize], mesh.positions[n as usize]);
                if through < distance[n as usize] && through < reach {
                    distance[n as usize] = through;
                    queue.push((Reverse(micro(through)), n));
                }
            }
        }
    }
    for (weight, distance) in weights.iter_mut().zip(&distance) {
        if *distance < reach {
            *weight *= smooth(distance / reach);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normals::{Weighting, vertex_normals};
    use crate::shapes;
    use crate::subdivide::{Limits, subdivide};

    struct Refined {
        source: Mesh,
        mesh: Mesh,
        parent: Vec<u32>,
        normals: Vec<P3>,
    }

    fn cube(max_edge: f32, mask: &Mask) -> (Refined, Vec<bool>) {
        let source = shapes::cuboid([10.0; 3]);
        let active = mask.active(&source);
        let fine = subdivide(&source, Limits { max_edge, max_triangles: usize::MAX, refine: Some(&active) });
        let normals = vertex_normals(&fine.mesh, Weighting::Angle);
        (Refined { source, mesh: fine.mesh, parent: fine.parent, normals }, active)
    }

    fn weights(mask: &Mask, max_edge: f32) -> (Refined, Vec<f32>) {
        let (refined, active) = cube(max_edge, mask);
        let bvh = Bvh::build(&refined.mesh);
        let weights = mask.weights(&active, &refined.mesh, &refined.parent, &refined.normals, None, Some(&bvh));
        (refined, weights)
    }

    #[test]
    fn the_bottom_stays_out_and_flat_to_its_rim() {
        let mask = Mask::without_bottom(5.0);
        let (refined, weights) = weights(&mask, 1.0);
        assert_eq!(mask.active(&refined.source), [vec![false; 2], vec![true; 10]].concat());
        for (p, w) in refined.mesh.positions.iter().zip(&weights) {
            assert_eq!(*w, if p[2] == 0.0 { 0.0 } else { 1.0 }, "{p:?}");
        }
        // The bottom was not even refined.
        assert_eq!(refined.parent.iter().filter(|&&p| p < 2).count(), 2 + 4 * 15);

        // Top and bottom, and at 90 degrees the walls too: nothing is left.
        let both = Mask { top_degrees: 5.0, bottom_degrees: 5.0, ..Mask::default() };
        assert_eq!(both.active(&refined.source).iter().filter(|a| **a).count(), 8);
        let all = Mask { top_degrees: 90.0, bottom_degrees: 90.0, ..Mask::default() };
        assert!(!all.active(&refined.source).contains(&true));
    }

    #[test]
    fn picked_faces_decide() {
        // The right side (triangles 6 and 7) is left out.
        let mut faces = vec![0; 12];
        faces[6] = EXCLUDED;
        faces[7] = EXCLUDED;
        let (refined, w) = weights(&Mask { faces, ..Mask::default() }, 1.0);
        for (p, w) in refined.mesh.positions.iter().zip(&w) {
            assert_eq!(*w, if p[0] == 10.0 { 0.0 } else { 1.0 });
        }
        // Only the top (triangles 2 and 3) is picked: its inside moves, its rim does not.
        let mut faces = vec![0; 12];
        faces[2] = INCLUDED;
        faces[3] = INCLUDED;
        let mask = Mask { faces, ..Mask::default() };
        let (refined, w) = weights(&mask, 1.0);
        assert_eq!(mask.active(&refined.source).iter().filter(|a| **a).count(), 2);
        for (p, w) in refined.mesh.positions.iter().zip(&w) {
            let inside_top = p[2] == 10.0 && p[0] > 0.0 && p[0] < 10.0 && p[1] > 0.0 && p[1] < 10.0;
            assert_eq!(*w, if inside_top { 1.0 } else { 0.0 }, "{p:?}");
        }
    }

    #[test]
    fn a_brush_marks_a_round_patch_on_the_side_it_touches() {
        let dab = Dab {
            centre: [5.0, 5.0, 10.0],
            normal: [0.0, 0.0, 1.0],
            radius: 3.0,
            hardness: 0.5,
            kind: DabKind::Exclude,
            erase: false,
        };
        let (refined, w) = weights(&Mask { dabs: vec![dab], ..Mask::default() }, 0.5);
        for (p, w) in refined.mesh.positions.iter().zip(&w) {
            let distance = vec::distance(*p, dab.centre);
            if distance <= 1.5 {
                assert_eq!(*w, 0.0);
            } else if distance >= 3.0 {
                assert_eq!(*w, 1.0);
            } else {
                assert!(*w > 0.0 && *w < 1.0, "{distance}: {w}");
            }
        }
        // Erasing half of it again brings the texture back there.
        let back = Dab { centre: [3.5, 5.0, 10.0], erase: true, hardness: 1.0, radius: 1.4, ..dab };
        let (refined, w) = weights(&Mask { dabs: vec![dab, back], ..Mask::default() }, 0.5);
        let at = |x: f32| {
            let v = refined.mesh.positions.iter().position(|p| *p == [x, 5.0, 10.0]).unwrap();
            w[v]
        };
        assert_eq!((at(3.75), at(5.0), at(6.25)), (1.0, 0.0, 0.0));

        // Painting texture in: only the patch moves, and only on the face it was painted on.
        let paint = Dab { kind: DabKind::Include, hardness: 1.0, centre: [5.0, 5.0, 10.0], radius: 3.0, ..dab };
        let thin = Mask { dabs: vec![Dab { centre: [5.0, 5.0, 0.2], ..paint }], ..Mask::default() };
        let (refined, w) = weights(&thin, 0.5);
        for (p, w) in refined.mesh.positions.iter().zip(&w) {
            // Close to the bottom, but the brush faced up: the bottom, facing down, gets nothing.
            assert_eq!(*w, 0.0, "{p:?}");
        }
        let (refined, w) = weights(&Mask { dabs: vec![paint], ..Mask::default() }, 0.5);
        for (p, w) in refined.mesh.positions.iter().zip(&w) {
            let inside = p[2] == 10.0 && vec::distance(*p, paint.centre) <= 3.0;
            assert_eq!(*w, if inside { 1.0 } else { 0.0 }, "{p:?}");
        }
    }

    #[test]
    fn a_mask_that_starts_empty_textures_nothing_until_something_is_picked() {
        let empty = Mask { only_picked: true, ..Mask::default() };
        let (refined, w) = weights(&empty, 2.0);
        assert!(!empty.active(&refined.source).contains(&true));
        assert!(w.iter().all(|w| *w == 0.0));
        let mut faces = vec![0; 12];
        faces[2] = INCLUDED;
        faces[3] = INCLUDED;
        let top = Mask { faces, only_picked: true, ..Mask::default() };
        assert_eq!(top.active(&refined.source).iter().filter(|a| **a).count(), 2);
    }

    #[test]
    fn a_click_fills_the_face_it_meant_whatever_the_resolution() {
        let coarse = shapes::cuboid([10.0; 3]);
        // A click on the top, right at the edge to the front, a hair above the surface.
        let click = Op::Fill {
            point: [5.0, 0.01, 10.02],
            normal: [0.0, 0.0, 1.0],
            kind: DabKind::Exclude,
            erase: false,
            degrees: 30.0,
        };
        let mut mask = Mask::default();
        mask.apply_ops(&coarse, &[click], 1.0);
        assert_eq!(mask.faces, [vec![0; 2], vec![EXCLUDED; 2], vec![0; 8]].concat());
        // The same click on a finer copy of the cube marks its whole top there as well.
        let fine = subdivide(&coarse, Limits { max_edge: 1.5, max_triangles: usize::MAX, refine: None });
        let mut mask = Mask::default();
        mask.apply_ops(&fine.mesh, &[click], 1.0);
        for (t, mark) in mask.faces.iter().enumerate() {
            assert_eq!(*mark, if fine.parent[t] == 2 || fine.parent[t] == 3 { EXCLUDED } else { 0 }, "triangle {t}");
        }
        // Facing the front instead, the same point means the front.
        let front = Op::Fill {
            point: [5.0, 0.01, 10.02],
            normal: [0.0, -1.0, 0.0],
            kind: DabKind::Include,
            erase: false,
            degrees: 30.0,
        };
        let mut mask = Mask::default();
        mask.apply_ops(&coarse, &[front], 1.0);
        assert_eq!(mask.faces, [vec![0; 4], vec![INCLUDED; 2], vec![0; 6]].concat());
    }

    #[test]
    fn strokes_count_in_their_order_and_far_clicks_are_not_meant() {
        let cube = shapes::cuboid([10.0; 3]);
        let top =
            |kind, erase| Op::Fill { point: [5.0, 5.0, 10.0], normal: [0.0, 0.0, 1.0], kind, erase, degrees: 30.0 };
        let dab = Dab {
            centre: [5.0, 5.0, 10.0],
            normal: [0.0, 0.0, 1.0],
            radius: 2.0,
            hardness: 1.0,
            kind: DabKind::Exclude,
            erase: false,
        };
        // Erasing takes a mark off that was there before the strokes (the inside found by itself).
        let mut mask = Mask { faces: vec![EXCLUDED; 12], ..Mask::default() };
        mask.apply_ops(&cube, &[top(DabKind::Exclude, true)], 1.0);
        assert_eq!(mask.faces, [vec![EXCLUDED; 2], vec![0; 2], vec![EXCLUDED; 8]].concat());
        // Clearing forgets what came before it, and only that.
        let mut mask = Mask::default();
        mask.apply_ops(&cube, &[top(DabKind::Exclude, false), Op::Dab(dab), Op::Clear, Op::Dab(dab)], 1.0);
        assert!(mask.faces.iter().all(|m| *m == 0));
        assert_eq!(mask.dabs, vec![dab]);
        // A click 30 mm away belongs to another part.
        let far = Op::Fill {
            point: [5.0, 5.0, 40.0],
            normal: [0.0, 0.0, 1.0],
            kind: DabKind::Exclude,
            erase: false,
            degrees: 30.0,
        };
        let mut mask = Mask::default();
        mask.apply_ops(&cube, &[far], 1.0);
        assert!(mask.faces.iter().all(|m| *m == 0));
    }

    #[test]
    fn the_texture_runs_out_towards_what_stays() {
        let mask = Mask { falloff: 3.0, ..Mask::without_bottom(5.0) };
        let (refined, w) = weights(&mask, 0.5);
        // On a wall the way along the edges to the bottom is the height.
        let at = |z: f32| {
            let v = refined.mesh.positions.iter().position(|p| *p == [0.0, 5.0, z]).unwrap();
            w[v]
        };
        assert_eq!((at(0.0), at(3.125), at(10.0)), (0.0, 1.0, 1.0));
        assert!((at(1.25) - smooth(1.25 / 3.0)).abs() < 0.03, "{}", at(1.25));
        assert!(at(0.625) < at(1.25) && at(1.25) < at(2.5) && at(2.5) < 1.0);

        // Held vertices count as staying as well.
        let (refined, active) = cube(0.5, &Mask::default());
        let held: Vec<bool> = refined.mesh.positions.iter().map(|p| p[2] == 10.0).collect();
        let mask = Mask { falloff: 2.0, ..Mask::default() };
        let w = mask.weights(&active, &refined.mesh, &refined.parent, &refined.normals, Some(&held), None);
        let v = refined.mesh.positions.iter().position(|p| *p == [0.0, 5.0, 9.375]).unwrap();
        assert!(w[v] > 0.0 && w[v] < 0.5);
    }
}
