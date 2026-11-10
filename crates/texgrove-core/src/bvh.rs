//! A tree of boxes over the triangles of a mesh, to find quickly what a ray hits and which
//! triangles lie near a point (a bounding volume hierarchy).
//!
//! The tree keeps no copy of the mesh: queries take the mesh it was built for.

use crate::mesh::{Bounds, Mesh};
use crate::vec::{self, P3};

/// Triangles per leaf at most, as long as they can be told apart by where they lie.
const LEAF: usize = 4;
/// Slices the tree tries to cut a box into, per axis.
const BINS: usize = 16;

#[derive(Debug, Clone, Copy)]
struct Node {
    bounds: Bounds,
    /// A leaf: its first triangle in `order`. Otherwise its first child; the second follows it.
    first: u32,
    /// The number of triangles of a leaf; 0 for a node with children.
    count: u32,
}

/// Where a ray meets a triangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// The distance along the ray, in lengths of its direction.
    pub t: f32,
    pub triangle: u32,
    /// The weights of the triangle's second and third corner at the hit (the first has `1 − u − v`).
    pub u: f32,
    pub v: f32,
}

/// A bounding volume hierarchy over a mesh's triangles.
#[derive(Debug, Clone, Default)]
pub struct Bvh {
    nodes: Vec<Node>,
    order: Vec<u32>,
}

impl Bvh {
    /// Builds the tree: boxes are cut where the surface of the two halves, weighted by their
    /// triangles, is smallest (the surface area heuristic, tried at [`BINS`] places per axis).
    pub fn build(mesh: &Mesh) -> Bvh {
        let count = mesh.triangles.len();
        let mut boxes = Vec::with_capacity(count);
        let mut centres = Vec::with_capacity(count);
        for t in 0..count {
            let mut bounds = Bounds::EMPTY;
            for p in mesh.corners(t) {
                bounds.grow(p);
            }
            centres.push(bounds.centre());
            boxes.push(bounds);
        }
        let mut bvh = Bvh { nodes: Vec::with_capacity(count / 2 + 1), order: (0..count as u32).collect() };
        if count == 0 {
            return bvh;
        }
        bvh.nodes.push(Node { bounds: Bounds::EMPTY, first: 0, count: count as u32 });
        let mut todo = vec![0usize];
        while let Some(index) = todo.pop() {
            let (start, len) = (bvh.nodes[index].first as usize, bvh.nodes[index].count as usize);
            let range = start..start + len;
            let mut bounds = Bounds::EMPTY;
            let mut centre_bounds = Bounds::EMPTY;
            for &t in &bvh.order[range.clone()] {
                bounds.join(&boxes[t as usize]);
                centre_bounds.grow(centres[t as usize]);
            }
            bvh.nodes[index].bounds = bounds;
            if len <= LEAF {
                continue;
            }
            let middle = match best_cut(&bvh.order[range.clone()], &boxes, &centres, &centre_bounds) {
                Some(cut) => {
                    let part = &mut bvh.order[range];
                    let mut left = 0;
                    for i in 0..part.len() {
                        if cut.bin_of(centres[part[i] as usize]) < cut.bin {
                            part.swap(i, left);
                            left += 1;
                        }
                    }
                    left
                }
                // The centres all lie at one place: halve by number, so leaves stay small.
                None => len / 2,
            };
            if middle == 0 || middle == len {
                continue; // cannot happen with a cut from best_cut; a larger leaf is still right
            }
            let child = bvh.nodes.len();
            bvh.nodes.push(Node { bounds: Bounds::EMPTY, first: start as u32, count: middle as u32 });
            bvh.nodes.push(Node {
                bounds: Bounds::EMPTY,
                first: (start + middle) as u32,
                count: (len - middle) as u32,
            });
            bvh.nodes[index].first = child as u32;
            bvh.nodes[index].count = 0;
            todo.push(child);
            todo.push(child + 1);
        }
        bvh
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The nearest triangle a ray hits between `nearest` and `farthest` (in lengths of
    /// `direction`), whichever side it is seen from.
    pub fn ray(&self, mesh: &Mesh, origin: P3, direction: P3, nearest: f32, farthest: f32) -> Option<Hit> {
        let mut best: Option<Hit> = None;
        let mut limit = farthest;
        self.walk_ray(origin, direction, nearest, &mut limit, |triangle, limit| {
            if let Some(hit) = ray_triangle(mesh, triangle, origin, direction)
                && hit.t >= nearest
                && hit.t <= *limit
            {
                *limit = hit.t;
                best = Some(hit);
            }
            false
        });
        best
    }

    /// Whether anything is in the way of a ray between `nearest` and `farthest`.
    pub fn blocked(&self, mesh: &Mesh, origin: P3, direction: P3, nearest: f32, farthest: f32) -> bool {
        let mut limit = farthest;
        self.walk_ray(origin, direction, nearest, &mut limit, |triangle, limit| {
            ray_triangle(mesh, triangle, origin, direction).is_some_and(|hit| hit.t >= nearest && hit.t <= *limit)
        })
    }

    /// Visits the triangles of all leaves a ray passes, nearer boxes first; `visit` may shorten
    /// the ray through `limit` and ends the walk by returning `true`.
    fn walk_ray(
        &self,
        origin: P3,
        direction: P3,
        nearest: f32,
        limit: &mut f32,
        mut visit: impl FnMut(u32, &mut f32) -> bool,
    ) -> bool {
        if self.nodes.is_empty() {
            return false;
        }
        let inverse = direction.map(|d| 1.0 / d);
        let mut stack = Vec::with_capacity(64);
        stack.push(0u32);
        while let Some(index) = stack.pop() {
            let node = &self.nodes[index as usize];
            if ray_box(&node.bounds, origin, inverse, nearest, *limit).is_none() {
                continue;
            }
            if node.count > 0 {
                for &t in &self.order[node.first as usize..(node.first + node.count) as usize] {
                    if visit(t, limit) {
                        return true;
                    }
                }
                continue;
            }
            let (a, b) = (node.first, node.first + 1);
            let enter_a = ray_box(&self.nodes[a as usize].bounds, origin, inverse, nearest, *limit);
            let enter_b = ray_box(&self.nodes[b as usize].bounds, origin, inverse, nearest, *limit);
            match (enter_a, enter_b) {
                // The nearer child is looked at first: pushed last.
                (Some(ta), Some(tb)) if ta <= tb => stack.extend([b, a]),
                (Some(_), Some(_)) => stack.extend([a, b]),
                (Some(_), None) => stack.push(a),
                (None, Some(_)) => stack.push(b),
                (None, None) => {}
            }
        }
        false
    }

    /// Adds to `found` every triangle with a point within `radius` of `centre`, in no
    /// particular order.
    pub fn within(&self, mesh: &Mesh, centre: P3, radius: f32, found: &mut Vec<u32>) {
        if self.nodes.is_empty() {
            return;
        }
        let limit = radius * radius;
        let mut stack = vec![0u32];
        while let Some(index) = stack.pop() {
            let node = &self.nodes[index as usize];
            if box_distance_squared(&node.bounds, centre) > limit {
                continue;
            }
            if node.count == 0 {
                stack.extend([node.first, node.first + 1]);
                continue;
            }
            for &t in &self.order[node.first as usize..(node.first + node.count) as usize] {
                let [a, b, c] = mesh.corners(t as usize);
                let nearest = vec::closest_on_triangle(centre, a, b, c);
                let d = vec::sub(nearest, centre);
                if vec::dot(d, d) <= limit {
                    found.push(t);
                }
            }
        }
    }

    /// The point of the surface nearest to `p`, with its triangle: `(triangle, point, distance)`.
    pub fn closest(&self, mesh: &Mesh, p: P3) -> Option<(u32, P3, f32)> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut best: Option<(u32, P3, f32)> = None;
        let mut limit = f32::INFINITY;
        let mut stack = vec![0u32];
        while let Some(index) = stack.pop() {
            let node = &self.nodes[index as usize];
            if box_distance_squared(&node.bounds, p) > limit {
                continue;
            }
            if node.count == 0 {
                let (a, b) = (node.first, node.first + 1);
                let da = box_distance_squared(&self.nodes[a as usize].bounds, p);
                let db = box_distance_squared(&self.nodes[b as usize].bounds, p);
                if da <= db {
                    stack.extend([b, a])
                } else {
                    stack.extend([a, b])
                }
                continue;
            }
            for &t in &self.order[node.first as usize..(node.first + node.count) as usize] {
                let [a, b, c] = mesh.corners(t as usize);
                let nearest = vec::closest_on_triangle(p, a, b, c);
                let d = vec::sub(nearest, p);
                let squared = vec::dot(d, d);
                if squared < limit {
                    limit = squared;
                    best = Some((t, nearest, squared.sqrt()));
                }
            }
        }
        best
    }
}

/// A cut through the triangles of a node: the centres are sorted into [`BINS`] slices along
/// `axis`, and those in slices below `bin` go left.
struct Cut {
    axis: usize,
    bin: usize,
    min: f32,
    scale: f32,
}

impl Cut {
    #[inline]
    fn bin_of(&self, centre: P3) -> usize {
        (((centre[self.axis] - self.min) * self.scale) as usize).min(BINS - 1)
    }
}

/// The cheapest cut for a node's triangles; `None` when all centres lie at one place.
fn best_cut(order: &[u32], boxes: &[Bounds], centres: &[P3], centre_bounds: &Bounds) -> Option<Cut> {
    let size = centre_bounds.size();
    let mut best: Option<(f32, Cut)> = None;
    for (axis, &extent) in size.iter().enumerate() {
        if extent <= 0.0 {
            continue;
        }
        let slices = Cut { axis, bin: 0, min: centre_bounds.min[axis], scale: BINS as f32 / extent };
        let mut counts = [0u32; BINS];
        let mut bounds = [Bounds::EMPTY; BINS];
        for &t in order {
            let bin = slices.bin_of(centres[t as usize]);
            counts[bin] += 1;
            bounds[bin].join(&boxes[t as usize]);
        }
        // The cost of cutting before slice `i`: each half's surface times its triangles. Sweep
        // from the right first, then from the left.
        let mut right_cost = [0.0f32; BINS];
        let (mut grown, mut n) = (Bounds::EMPTY, 0u32);
        for i in (1..BINS).rev() {
            grown.join(&bounds[i]);
            n += counts[i];
            right_cost[i] = if n == 0 { 0.0 } else { grown.half_area() * n as f32 };
        }
        let (mut grown, mut n) = (Bounds::EMPTY, 0u32);
        for i in 1..BINS {
            grown.join(&bounds[i - 1]);
            n += counts[i - 1];
            if n == 0 || n as usize == order.len() {
                continue; // everything on one side is no cut
            }
            let cost = grown.half_area() * n as f32 + right_cost[i];
            if best.as_ref().is_none_or(|(c, _)| cost < *c) {
                best = Some((cost, Cut { axis, bin: i, min: slices.min, scale: slices.scale }));
            }
        }
    }
    best.map(|(_, cut)| cut)
}

/// Where a ray enters a box, if it passes it between `nearest` and `farthest`.
#[inline]
fn ray_box(bounds: &Bounds, origin: P3, inverse: P3, nearest: f32, farthest: f32) -> Option<f32> {
    let (mut enter, mut leave) = (nearest, farthest);
    for axis in 0..3 {
        // A ray that does not move along this axis is between the two faces all the way or
        // never. Worked out with 0 × ∞ instead, "never" came out as "enters at infinity", which
        // an endless ray took for a pass, and such a ray walked half the tree.
        if inverse[axis].is_infinite() {
            if origin[axis] < bounds.min[axis] || origin[axis] > bounds.max[axis] {
                return None;
            }
            continue;
        }
        let t1 = (bounds.min[axis] - origin[axis]) * inverse[axis];
        let t2 = (bounds.max[axis] - origin[axis]) * inverse[axis];
        enter = enter.max(t1.min(t2));
        leave = leave.min(t1.max(t2));
    }
    // A hair of slack: a triangle lying flat in a face of its box must not be lost to rounding.
    (enter <= leave + 4.0 * f32::EPSILON * enter.abs()).then_some(enter)
}

#[inline]
fn box_distance_squared(bounds: &Bounds, p: P3) -> f32 {
    let outside = [0, 1, 2].map(|axis| (bounds.min[axis] - p[axis]).max(p[axis] - bounds.max[axis]).max(0.0));
    vec::dot(outside, outside)
}

/// Where a ray meets a triangle's plane inside the triangle, from either side (Möller–Trumbore).
/// The distance may be negative: behind the origin.
pub fn ray_triangle(mesh: &Mesh, triangle: u32, origin: P3, direction: P3) -> Option<Hit> {
    let [a, b, c] = mesh.corners(triangle as usize);
    let (edge1, edge2) = (vec::sub(b, a), vec::sub(c, a));
    let p = vec::cross(direction, edge2);
    let det = vec::dot(edge1, p);
    // Parallel to the triangle, measured against the sizes involved.
    if det.abs() <= 1e-12 * vec::length(edge1) * vec::length(edge2) * vec::length(direction) {
        return None;
    }
    let inverse = 1.0 / det;
    let from_a = vec::sub(origin, a);
    let u = vec::dot(from_a, p) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = vec::cross(from_a, edge1);
    let v = vec::dot(direction, q) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    Some(Hit { t: vec::dot(edge2, q) * inverse, triangle, u, v })
}

/// [`Bvh::ray`] by trying every triangle: for small meshes and to check the tree.
pub fn ray_slowly(mesh: &Mesh, origin: P3, direction: P3, nearest: f32, farthest: f32) -> Option<Hit> {
    (0..mesh.triangles.len() as u32)
        .filter_map(|t| ray_triangle(mesh, t, origin, direction))
        .filter(|hit| hit.t >= nearest && hit.t <= farthest)
        .min_by(|a, b| a.t.total_cmp(&b.t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ray_along_an_axis_misses_the_boxes_beside_it() {
        let bounds = Bounds { min: [0.0, 0.0, 0.0], max: [1.0, 1.0, 1.0] };
        let along_x = [1.0f32, 0.0, 0.0].map(|d| 1.0 / d);
        // Through the box, with and without an end to the ray.
        assert!(ray_box(&bounds, [-1.0, 0.5, 0.5], along_x, 0.0, f32::INFINITY).is_some());
        assert!(ray_box(&bounds, [-1.0, 0.5, 0.5], along_x, 0.0, 10.0).is_some());
        // Along one of its faces counts as through it.
        assert!(ray_box(&bounds, [-1.0, 0.0, 0.5], along_x, 0.0, f32::INFINITY).is_some());
        // Beside it, on either side: no end to the ray must not turn that into a pass.
        for origin in [[-1.0, 2.0, 0.5], [-1.0, -2.0, 0.5], [-1.0, 0.5, 2.0], [-1.0, 0.5, -2.0]] {
            assert!(ray_box(&bounds, origin, along_x, 0.0, f32::INFINITY).is_none(), "{origin:?}");
            assert!(ray_box(&bounds, origin, along_x, 0.0, 10.0).is_none(), "{origin:?}");
        }
    }
    use crate::shapes;

    /// Numbers between 0 and 1 that are the same on every run.
    struct Dice(u64);

    impl Dice {
        fn roll(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 40) as f32 / (1u64 << 24) as f32
        }

        fn point(&mut self, reach: f32) -> P3 {
            [0, 1, 2].map(|_| (self.roll() * 2.0 - 1.0) * reach)
        }
    }

    #[test]
    fn the_tree_finds_what_trying_every_triangle_finds() {
        let ring = shapes::torus(20.0, 6.0, 96, 48);
        let bvh = Bvh::build(&ring);
        let mut dice = Dice(7);
        let mut hits = 0;
        for _ in 0..2000 {
            let origin = dice.point(40.0);
            let direction = vec::sub(dice.point(22.0), origin);
            let slow = ray_slowly(&ring, origin, direction, 0.0, f32::INFINITY);
            let fast = bvh.ray(&ring, origin, direction, 0.0, f32::INFINITY);
            assert_eq!(slow.map(|h| h.t), fast.map(|h| h.t), "from {origin:?} along {direction:?}");
            assert_eq!(bvh.blocked(&ring, origin, direction, 0.0, f32::INFINITY), slow.is_some());
            hits += usize::from(slow.is_some());
        }
        assert!(hits > 400, "only {hits} of the rays hit the ring");
    }

    #[test]
    fn a_hit_knows_its_place_in_the_triangle() {
        let cube = shapes::cuboid([10.0; 3]);
        let bvh = Bvh::build(&cube);
        let hit = bvh.ray(&cube, [2.0, 3.0, 50.0], [0.0, 0.0, -1.0], 0.0, f32::INFINITY).unwrap();
        assert_eq!(hit.t, 40.0);
        let [a, b, c] = cube.corners(hit.triangle as usize);
        let point = vec::add(vec::scale(a, 1.0 - hit.u - hit.v), vec::add(vec::scale(b, hit.u), vec::scale(c, hit.v)));
        assert!(vec::distance(point, [2.0, 3.0, 10.0]) < 1e-5);
        // From inside, the far wall; limited to a stretch that ends before it, nothing.
        assert_eq!(bvh.ray(&cube, [5.0; 3], [1.0, 0.0, 0.0], 0.0, f32::INFINITY).unwrap().t, 5.0);
        assert!(bvh.ray(&cube, [5.0; 3], [1.0, 0.0, 0.0], 0.0, 4.0).is_none());
        assert!(!bvh.blocked(&cube, [5.0; 3], [1.0, 0.0, 0.0], 0.0, 4.0));
        // Along a face of the boxes: no NaN trouble.
        assert!(bvh.ray(&cube, [-5.0, 0.0, 5.0], [1.0, 0.0, 0.0], 0.0, f32::INFINITY).is_some());
    }

    #[test]
    fn triangles_near_a_point_and_the_nearest_one() {
        let ball = shapes::sphere(10.0, 64, 32);
        let bvh = Bvh::build(&ball);
        let mut dice = Dice(3);
        for _ in 0..200 {
            let centre = dice.point(12.0);
            let radius = 0.5 + dice.roll() * 4.0;
            let mut found = Vec::new();
            bvh.within(&ball, centre, radius, &mut found);
            found.sort_unstable();
            let slow: Vec<u32> = (0..ball.triangles.len() as u32)
                .filter(|&t| {
                    let [a, b, c] = ball.corners(t as usize);
                    vec::distance(vec::closest_on_triangle(centre, a, b, c), centre) <= radius
                })
                .collect();
            assert_eq!(found, slow);

            let (_, point, distance) = bvh.closest(&ball, centre).unwrap();
            let slowest = (0..ball.triangles.len())
                .map(|t| {
                    let [a, b, c] = ball.corners(t);
                    vec::distance(vec::closest_on_triangle(centre, a, b, c), centre)
                })
                .fold(f32::INFINITY, f32::min);
            assert_eq!(distance, slowest);
            assert!((vec::distance(point, centre) - distance).abs() < 1e-5);
        }
    }

    #[test]
    fn empty_and_piled_up_meshes_work() {
        let empty = Bvh::build(&Mesh::default());
        assert!(empty.is_empty());
        assert!(empty.ray(&Mesh::default(), [0.0; 3], [1.0, 0.0, 0.0], 0.0, 1.0).is_none());
        assert!(empty.closest(&Mesh::default(), [0.0; 3]).is_none());

        // Forty copies of one triangle: their centres cannot be told apart.
        let one = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let pile = Mesh::from_soup(&[one; 40]);
        let bvh = Bvh::build(&pile);
        assert!(bvh.nodes.iter().all(|n| n.count as usize <= LEAF));
        assert_eq!(bvh.ray(&pile, [0.2, 0.2, 1.0], [0.0, 0.0, -1.0], 0.0, 9.0).unwrap().t, 1.0);
        let mut found = Vec::new();
        bvh.within(&pile, [0.2, 0.2, 0.1], 0.2, &mut found);
        assert_eq!(found.len(), 40);
    }
}
