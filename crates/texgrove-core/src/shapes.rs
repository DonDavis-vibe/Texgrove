//! Simple closed and open shapes, for tests, benchmarks and examples.

use crate::mesh::Mesh;
use crate::vec::P3;
use core::f32::consts::{PI, TAU};

/// A box from the origin to `size`, its faces facing outwards: 8 vertices, 12 triangles.
pub fn cuboid(size: P3) -> Mesh {
    let [x, y, z] = size;
    let positions = vec![
        [0.0, 0.0, 0.0],
        [x, 0.0, 0.0],
        [x, y, 0.0],
        [0.0, y, 0.0],
        [0.0, 0.0, z],
        [x, 0.0, z],
        [x, y, z],
        [0.0, y, z],
    ];
    #[rustfmt::skip]
    let triangles = vec![
        [0, 2, 1], [0, 3, 2], // bottom
        [4, 5, 6], [4, 6, 7], // top
        [0, 1, 5], [0, 5, 4], // front
        [1, 2, 6], [1, 6, 5], // right
        [2, 3, 7], [2, 7, 6], // back
        [3, 0, 4], [3, 4, 7], // left
    ];
    Mesh { positions, triangles }
}

/// A sphere round the origin with `segments` round the equator and `rings` from pole to pole.
pub fn sphere(radius: f32, segments: u32, rings: u32) -> Mesh {
    let (segments, rings) = (segments.max(3), rings.max(2));
    let mut mesh = Mesh::default();
    mesh.positions.push([0.0, 0.0, -radius]);
    for r in 1..rings {
        let polar = PI * r as f32 / rings as f32;
        let (z, ring_radius) = (-radius * libm::cosf(polar), radius * libm::sinf(polar));
        for s in 0..segments {
            let around = TAU * s as f32 / segments as f32;
            mesh.positions.push([ring_radius * libm::cosf(around), ring_radius * libm::sinf(around), z]);
        }
    }
    mesh.positions.push([0.0, 0.0, radius]);
    let top = mesh.positions.len() as u32 - 1;
    let at = |r: u32, s: u32| 1 + (r - 1) * segments + s % segments;
    for s in 0..segments {
        mesh.triangles.push([0, at(1, s + 1), at(1, s)]);
        for r in 1..rings - 1 {
            mesh.triangles.push([at(r, s), at(r, s + 1), at(r + 1, s + 1)]);
            mesh.triangles.push([at(r, s), at(r + 1, s + 1), at(r + 1, s)]);
        }
        mesh.triangles.push([top, at(rings - 1, s), at(rings - 1, s + 1)]);
    }
    mesh
}

/// A torus round the z axis: `major` is the radius of the ring, `minor` that of the tube.
pub fn torus(major: f32, minor: f32, segments: u32, sides: u32) -> Mesh {
    let (segments, sides) = (segments.max(3), sides.max(3));
    let mut mesh = Mesh::default();
    for s in 0..segments {
        let around = TAU * s as f32 / segments as f32;
        for t in 0..sides {
            let tube = TAU * t as f32 / sides as f32;
            let reach = major + minor * libm::cosf(tube);
            mesh.positions.push([reach * libm::cosf(around), reach * libm::sinf(around), minor * libm::sinf(tube)]);
        }
    }
    let at = |s: u32, t: u32| (s % segments) * sides + t % sides;
    for s in 0..segments {
        for t in 0..sides {
            mesh.triangles.push([at(s, t), at(s + 1, t), at(s + 1, t + 1)]);
            mesh.triangles.push([at(s, t), at(s + 1, t + 1), at(s, t + 1)]);
        }
    }
    mesh
}

/// A flat open sheet in the xy plane, `cells` squares each way, facing up.
pub fn sheet(size: [f32; 2], cells: [u32; 2]) -> Mesh {
    let [nx, ny] = cells.map(|c| c.max(1));
    let mut mesh = Mesh::default();
    for j in 0..=ny {
        for i in 0..=nx {
            mesh.positions.push([size[0] * i as f32 / nx as f32, size[1] * j as f32 / ny as f32, 0.0]);
        }
    }
    let at = |i: u32, j: u32| j * (nx + 1) + i;
    for j in 0..ny {
        for i in 0..nx {
            mesh.triangles.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
            mesh.triangles.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
        }
    }
    mesh
}

/// The mesh as an STL stores it: every triangle with three vertices of its own.
pub fn soup(mesh: &Mesh) -> Mesh {
    let triangles: Vec<[P3; 3]> = (0..mesh.triangles.len()).map(|t| mesh.corners(t)).collect();
    Mesh::from_soup(&triangles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_face_outwards() {
        let ball = sphere(10.0, 32, 16);
        let exact = 4.0 / 3.0 * core::f64::consts::PI * 1000.0;
        assert!(ball.volume() > 0.95 * exact && ball.volume() < exact, "{}", ball.volume());
        let ring = torus(20.0, 5.0, 48, 24);
        let exact = 2.0 * core::f64::consts::PI.powi(2) * 20.0 * 25.0;
        assert!(ring.volume() > 0.95 * exact && ring.volume() < exact, "{}", ring.volume());
        assert!((sheet([30.0, 20.0], [3, 2]).area() - 600.0).abs() < 1e-4);
        assert_eq!(soup(&cuboid([1.0; 3])).positions.len(), 36);
    }
}
