//! Which way the surface faces: per triangle, per vertex, and where it has a sharp edge.

use crate::mesh::Mesh;
use crate::topology::{EdgeKind, Topology};
use crate::vec::{self, P3};

/// The unit normal of every triangle; zero for a triangle without area.
pub fn face_normals(mesh: &Mesh) -> Vec<P3> {
    (0..mesh.triangles.len()).map(|t| face_normal(mesh, t)).collect()
}

/// The unit normal of triangle `t`; zero if it has no area.
#[inline]
pub fn face_normal(mesh: &Mesh, t: usize) -> P3 {
    let n = mesh.area_vector(t);
    let len = vec::length64(n);
    if len > 0.0 { [(n[0] / len) as f32, (n[1] / len) as f32, (n[2] / len) as f32] } else { [0.0; 3] }
}

/// How much each triangle counts for the normal of a vertex.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Weighting {
    /// By the angle the triangle has at the vertex. The result does not depend on how a face
    /// happens to be cut into triangles: on the edge of a box it is always the exact diagonal.
    #[default]
    Angle,
    /// By the triangle's area: cheaper, and fine on evenly triangulated round shapes.
    Area,
}

/// One unit normal per vertex, the mean of the triangles round it. A vertex without triangles,
/// or one where they cancel out, gets zero.
///
/// Texgrove moves every vertex along this one direction, whatever triangle it is seen from: so
/// the triangles round a vertex still meet after moving, and the mesh stays closed.
pub fn vertex_normals(mesh: &Mesh, weighting: Weighting) -> Vec<P3> {
    let mut sums = vec![[0.0f32; 3]; mesh.positions.len()];
    for (t, corners) in mesh.triangles.iter().enumerate() {
        let twice_area = mesh.area_vector(t);
        let len = vec::length64(twice_area);
        if len <= 0.0 {
            continue;
        }
        match weighting {
            Weighting::Area => {
                let n = twice_area.map(|c| c as f32);
                for &v in corners {
                    sums[v as usize] = vec::add(sums[v as usize], n);
                }
            }
            Weighting::Angle => {
                let unit = twice_area.map(|c| (c / len) as f32);
                let p = mesh.corners(t);
                for k in 0..3 {
                    let (to_next, to_previous) = (vec::sub(p[(k + 1) % 3], p[k]), vec::sub(p[(k + 2) % 3], p[k]));
                    let angle =
                        libm::atan2f(vec::length(vec::cross(to_next, to_previous)), vec::dot(to_next, to_previous));
                    let v = corners[k] as usize;
                    sums[v] = vec::add(sums[v], vec::scale(unit, angle));
                }
            }
        }
    }
    for n in &mut sums {
        *n = vec::normalize(*n);
    }
    sums
}

/// For every edge whether it is sharp: its two triangles differ by more than `degrees`, or it is
/// not an edge between exactly two triangles at all (a rim, a fin).
pub fn sharp_edges(mesh: &Mesh, topology: &Topology, face_normals: &[P3], degrees: f32) -> Vec<bool> {
    let limit = libm::cosf(degrees.to_radians());
    (0..topology.edge_count() as u32)
        .map(|edge| {
            if topology.kind(edge) != EdgeKind::Manifold {
                return true;
            }
            let mut sides = topology.edge_sides(edge);
            let (a, b) = (sides.next().unwrap(), sides.next().unwrap());
            // Triangles that disagree about the outside are compared as if one were turned.
            let turn = if a.vertices(mesh)[0] == b.vertices(mesh)[0] { -1.0 } else { 1.0 };
            turn * vec::dot(face_normals[a.triangle as usize], face_normals[b.triangle as usize]) < limit
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes;

    fn close(a: P3, b: P3, tolerance: f32) -> bool {
        vec::distance(a, b) <= tolerance
    }

    #[test]
    fn cube_faces_point_along_the_axes() {
        let cube = shapes::cuboid([2.0, 3.0, 4.0]);
        let normals = face_normals(&cube);
        assert_eq!(normals[0], [0.0, 0.0, -1.0]);
        assert_eq!(normals[2], [0.0, 0.0, 1.0]);
        assert_eq!(normals[4], [0.0, -1.0, 0.0]);
        assert_eq!(normals[6], [1.0, 0.0, 0.0]);
    }

    #[test]
    fn by_angle_a_cube_corner_points_along_the_diagonal() {
        // Each corner of this cube has one, two or three triangles per face: by area the normal
        // would lean towards the faces with more of them, by angle it does not.
        let cube = shapes::cuboid([1.0; 3]);
        let d = 1.0 / 3.0f32.sqrt();
        let by_angle = vertex_normals(&cube, Weighting::Angle);
        for (v, n) in by_angle.iter().enumerate() {
            let p = cube.positions[v];
            let expected = [0, 1, 2].map(|a| if p[a] > 0.5 { d } else { -d });
            assert!(close(*n, expected, 1e-6), "vertex {v}: {n:?}");
        }
        let by_area = vertex_normals(&cube, Weighting::Area);
        assert!(by_area.iter().zip(&by_angle).any(|(a, b)| !close(*a, *b, 1e-3)));
    }

    #[test]
    fn sphere_normals_point_away_from_the_centre() {
        let ball = shapes::sphere(5.0, 48, 24);
        for weighting in [Weighting::Angle, Weighting::Area] {
            for (n, p) in vertex_normals(&ball, weighting).iter().zip(&ball.positions) {
                // Next to the poles the rings are cut unevenly, which tilts a normal by a degree or two.
                assert!(close(*n, vec::normalize(*p), 0.04), "{n:?} at {p:?}");
            }
        }
    }

    #[test]
    fn a_cube_has_twelve_sharp_edges_and_a_sphere_none() {
        let cube = shapes::cuboid([1.0; 3]);
        let topology = Topology::build(&cube);
        let sharp = sharp_edges(&cube, &topology, &face_normals(&cube), 30.0);
        assert_eq!(sharp.iter().filter(|&&s| s).count(), 12); // the six diagonals are flat

        let ball = shapes::sphere(5.0, 48, 24);
        let topology = Topology::build(&ball);
        assert!(!sharp_edges(&ball, &topology, &face_normals(&ball), 30.0).contains(&true));

        // A rim counts as sharp, and a turned triangle does not make its flat neighbours sharp.
        let mut sheet = shapes::sheet([2.0, 2.0], [2, 2]);
        sheet.triangles[0].swap(1, 2);
        let topology = Topology::build(&sheet);
        let sharp = sharp_edges(&sheet, &topology, &face_normals(&sheet), 30.0);
        assert_eq!(sharp.iter().filter(|&&s| s).count(), 8);
    }
}
