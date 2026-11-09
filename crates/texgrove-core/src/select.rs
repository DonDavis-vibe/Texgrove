//! Picking faces of a model by what they are: lying open to the outside or tucked inside,
//! or belonging to one smooth surface.
//!
//! A part with an inside that has to fit something (a bore, a thread, a seat) must not get
//! texture there. These pick the triangles for a [`crate::mask::Mask`].

use crate::bvh::Bvh;
use crate::mesh::Mesh;
use crate::normals::face_normal;
use crate::topology::Topology;
use crate::vec::{self, P3};

/// Rays sent out from each triangle to see how much of the sky it has.
const RAYS: usize = 48;

/// For every triangle how open it lies: the part of the directions in front of it (weighted
/// towards straight ahead) in which nothing of the model is in the way. An outside wall has
/// nearly 1, the wall of a bore next to 0, the floor of a groove something between.
pub fn openness(mesh: &Mesh, bvh: &Bvh) -> Vec<f32> {
    let reach = mesh.bounds().diagonal().max(1e-6);
    let directions = fan();
    (0..mesh.triangles.len())
        .map(|t| {
            let normal = face_normal(mesh, t);
            if normal == [0.0; 3] {
                return 0.0;
            }
            let [a, b, c] = mesh.corners(t);
            let centre = vec::scale(vec::add(vec::add(a, b), c), 1.0 / 3.0);
            // A step off the surface, so the ray does not meet the triangle it starts from.
            let origin = vec::add(centre, vec::scale(normal, reach * 1e-5));
            let (right, forward) = frame(normal);
            let free = directions
                .iter()
                .filter(|d| {
                    let direction = vec::add(
                        vec::add(vec::scale(right, d[0]), vec::scale(forward, d[1])),
                        vec::scale(normal, d[2]),
                    );
                    !bvh.blocked(mesh, origin, direction, 0.0, f32::INFINITY)
                })
                .count();
            free as f32 / RAYS as f32
        })
        .collect()
}

/// For every triangle whether it belongs to an inside: a wall that looks across a hollow at
/// another wall, like the bore of a tube, the inside of a cup or a pocket.
///
/// Each triangle looks straight ahead; then every smooth surface (see [`surfaces`], with edges
/// of more than `degrees` between them) is inside or outside as a whole, by what most of its
/// area sees. So the wall of a wide, shallow bore is inside right up to its rim, and an outside
/// wall is not turned inside by a rib that stands in front of a part of it.
///
/// A groove on the outside is a hollow as well and comes out as inside: what a part's inside
/// is, in the end only its owner knows. This is a first pick, to be corrected by hand.
pub fn inside(mesh: &Mesh, topology: &Topology, bvh: &Bvh, degrees: f32) -> Vec<bool> {
    let reach = mesh.bounds().diagonal().max(1e-6);
    let labels = surfaces(mesh, topology, degrees);
    let count = labels.iter().max().map_or(0, |max| *max as usize + 1);
    // Per surface: the area that faces a wall, and all of its area.
    let mut facing = vec![0.0f64; count];
    let mut whole = vec![0.0f64; count];
    for t in 0..mesh.triangles.len() {
        let normal = face_normal(mesh, t);
        let area = vec::length64(mesh.area_vector(t));
        if normal == [0.0; 3] || area <= 0.0 {
            continue;
        }
        let [a, b, c] = mesh.corners(t);
        let centre = vec::scale(vec::add(vec::add(a, b), c), 1.0 / 3.0);
        let origin = vec::add(centre, vec::scale(normal, reach * 1e-5));
        whole[labels[t] as usize] += area;
        if bvh.blocked(mesh, origin, normal, 0.0, f32::INFINITY) {
            facing[labels[t] as usize] += area;
        }
    }
    labels.iter().map(|&label| facing[label as usize] > 0.5 * whole[label as usize]).collect()
}

/// A number for every triangle saying which smooth surface it belongs to: triangles have the
/// same number when one can be reached from the other over edges where the two faces differ by
/// no more than `degrees`. The numbers count up from 0 in the order of the triangles.
pub fn surfaces(mesh: &Mesh, topology: &Topology, degrees: f32) -> Vec<u32> {
    let mut labels = vec![u32::MAX; mesh.triangles.len()];
    let mut next = 0;
    for t in 0..mesh.triangles.len() as u32 {
        if labels[t as usize] == u32::MAX {
            for member in surface(mesh, topology, t, degrees) {
                labels[member as usize] = next;
            }
            next += 1;
        }
    }
    labels
}

/// The triangles of the smooth surface a triangle belongs to: all that can be reached from
/// `start` over edges where the two faces differ by no more than `degrees`. A click on a wall
/// picks the wall up to its sharp edges.
pub fn surface(mesh: &Mesh, topology: &Topology, start: u32, degrees: f32) -> Vec<u32> {
    let limit = libm::cosf(degrees.clamp(0.0, 180.0).to_radians()) - 1e-5;
    let mut reached = vec![false; mesh.triangles.len()];
    let mut found = Vec::new();
    if start as usize >= reached.len() {
        return found;
    }
    reached[start as usize] = true;
    let mut todo = vec![start];
    while let Some(t) = todo.pop() {
        found.push(t);
        let normal = face_normal(mesh, t as usize);
        for corner in 0..3 {
            // Over rims and fins there is no "next face" to speak of.
            let Some(other) = topology.opposite(t, corner) else { continue };
            if reached[other.triangle as usize] {
                continue;
            }
            // Each step is measured against the face it comes from, so a round wall is followed
            // all the way round even though its far side faces the other way.
            if vec::dot(normal, face_normal(mesh, other.triangle as usize)) >= limit {
                reached[other.triangle as usize] = true;
                todo.push(other.triangle);
            }
        }
    }
    found.sort_unstable();
    found
}

/// Directions over the half of the sky round "straight ahead" (the third axis), more of them
/// near it: the same on every run.
fn fan() -> [P3; RAYS] {
    let mut directions = [[0.0; 3]; RAYS];
    for (i, d) in directions.iter_mut().enumerate() {
        // A sunflower's spiral, with heights spaced so each ray stands for the same share of
        // the light a flat face would catch.
        let share = (i as f32 + 0.5) / RAYS as f32;
        let ahead = libm::sqrtf(1.0 - share);
        let aside = libm::sqrtf(share);
        let turn = i as f32 * 2.399_963_2; // the golden angle
        *d = [aside * libm::cosf(turn), aside * libm::sinf(turn), ahead];
    }
    directions
}

/// Two unit vectors at right angles to `normal` and to each other.
fn frame(normal: P3) -> (P3, P3) {
    let helper = if normal[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let right = vec::normalize(vec::cross(helper, normal));
    (right, vec::cross(normal, right))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes;
    use core::f32::consts::TAU;

    /// A tube standing on z = 0, open at both ends: outer wall, inner wall and two flat rings.
    fn tube(outer: f32, inner: f32, height: f32, segments: u32) -> Mesh {
        let mut mesh = Mesh::default();
        for s in 0..segments {
            let (sin, cos) =
                (libm::sinf(TAU * s as f32 / segments as f32), libm::cosf(TAU * s as f32 / segments as f32));
            for (radius, z) in [(outer, 0.0), (outer, height), (inner, height), (inner, 0.0)] {
                mesh.positions.push([radius * cos, radius * sin, z]);
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

    /// Which of the tube's four surfaces a triangle is: 0 outer wall, 1 top, 2 inner wall, 3 bottom.
    fn part(triangle: usize) -> usize {
        (triangle / 2) % 4
    }

    #[test]
    fn the_test_tube_is_a_sound_tube() {
        let tube = tube(20.0, 15.0, 60.0, 48);
        let report = crate::diagnostics::diagnose(&tube);
        assert!(report.sound() && report.euler() == 0, "{report:?}");
    }

    #[test]
    fn the_inside_of_a_tube_is_inside() {
        let tube = tube(20.0, 15.0, 60.0, 48);
        let bvh = Bvh::build(&tube);
        let open = openness(&tube, &bvh);
        for (t, open) in open.iter().enumerate() {
            match part(t) {
                0 | 1 | 3 => assert!(*open > 0.95, "triangle {t} of part {}: {open}", part(t)),
                _ => assert!(*open < 0.3, "triangle {t} of the bore: {open}"),
            }
        }
        // By what its walls face, the bore is inside as a whole and nothing else is, however
        // wide and shallow the tube.
        for (height, segments) in [(60.0, 48), (6.0, 48), (300.0, 12)] {
            let other = super::tests::tube(20.0, 15.0, height, segments);
            let within = inside(&other, &Topology::build(&other), &Bvh::build(&other), 30.0);
            for (t, within) in within.iter().enumerate() {
                assert_eq!(*within, part(t) == 2, "triangle {t} of a tube {height} high");
            }
        }
        // A closed box is outside all round; seen from inside nothing would be.
        let cube = shapes::cuboid([10.0; 3]);
        assert!(openness(&cube, &Bvh::build(&cube)).iter().all(|o| *o == 1.0));
        assert!(!inside(&cube, &Topology::build(&cube), &Bvh::build(&cube), 30.0).contains(&true));
    }

    #[test]
    fn a_click_picks_a_wall_up_to_its_edges() {
        let tube = tube(20.0, 15.0, 60.0, 48);
        let topology = Topology::build(&tube);
        // Segment 5's outer wall: the whole outer wall and nothing else.
        let wall = surface(&tube, &topology, 5 * 8, 30.0);
        assert_eq!(wall.len(), 96);
        assert!(wall.iter().all(|&t| part(t as usize) == 0));
        // The same from the inside.
        let bore = surface(&tube, &topology, 5 * 8 + 4, 30.0);
        assert_eq!(bore.len(), 96);
        assert!(bore.iter().all(|&t| part(t as usize) == 2));
        // With the angle wide open the click takes everything; with none, the flat piece only.
        assert_eq!(surface(&tube, &topology, 0, 180.0).len(), tube.triangles.len());
        assert_eq!(surface(&tube, &topology, 0, 0.0), vec![0, 1]);
        assert!(surface(&tube, &topology, 99_999, 30.0).is_empty());
        // All in all the tube has four surfaces, numbered as they come.
        let labels = surfaces(&tube, &topology, 30.0);
        for (t, label) in labels.iter().enumerate() {
            assert_eq!(*label as usize, part(t));
        }
    }
}
