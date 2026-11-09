//! Joining vertices that lie at the same place.
//!
//! An STL stores every triangle with three vertices of its own. Before anything can ask "which
//! triangles meet here", vertices at one place have to become one vertex.

use crate::mesh::{Bounds, Mesh};
use crate::vec::P3;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// A welded mesh and where its parts came from.
#[derive(Debug, Clone, Default)]
pub struct Welded {
    pub mesh: Mesh,
    /// For every triangle of `mesh`: its number in the mesh it was welded from.
    pub source_triangle: Vec<u32>,
    /// For every vertex of the mesh it was welded from: its number in `mesh`.
    pub vertex_map: Vec<u32>,
    /// Triangles left out because two of their corners fell together.
    pub collapsed: usize,
}

/// The distance below which two vertices of a model this large count as one: a hundred
/// thousandth of the box's diagonal, and never less than what `f32` can tell apart there.
pub fn default_tolerance(bounds: &Bounds) -> f32 {
    if bounds.is_empty() {
        return 1e-6;
    }
    let reach = bounds.min.iter().chain(&bounds.max).fold(0.0f32, |m, c| m.max(c.abs()));
    (bounds.diagonal() * 1e-5).max(reach * 4.0 * f32::EPSILON).max(1e-6)
}

/// Joins vertices closer than about `tolerance` and drops triangles that lose their area by it.
///
/// Vertices are sorted into cells of edge `tolerance`; the first vertex of a cell keeps its exact
/// place and the others of that cell become it. The order of vertices and triangles is kept.
pub fn weld(mesh: &Mesh, tolerance: f32) -> Welded {
    let tolerance = f64::from(tolerance.max(f32::MIN_POSITIVE));
    let cell = |p: P3| p.map(|c| (f64::from(c) / tolerance).round() as i64);
    let mut seen: HashMap<[i64; 3], u32, BuildHasherDefault<CellHasher>> =
        HashMap::with_capacity_and_hasher(mesh.positions.len() / 2, BuildHasherDefault::default());
    let mut out = Welded { vertex_map: Vec::with_capacity(mesh.positions.len()), ..Welded::default() };
    for &p in &mesh.positions {
        let next = out.mesh.positions.len() as u32;
        let v = *seen.entry(cell(p)).or_insert(next);
        if v == next {
            out.mesh.positions.push(p);
        }
        out.vertex_map.push(v);
    }
    out.mesh.triangles.reserve(mesh.triangles.len());
    out.source_triangle.reserve(mesh.triangles.len());
    for (i, t) in mesh.triangles.iter().enumerate() {
        let [a, b, c] = t.map(|v| out.vertex_map[v as usize]);
        if a == b || b == c || c == a {
            out.collapsed += 1;
            continue;
        }
        out.mesh.triangles.push([a, b, c]);
        out.source_triangle.push(i as u32);
    }
    out
}

/// [`weld`] with the [`default_tolerance`] of the mesh.
pub fn weld_default(mesh: &Mesh) -> Welded {
    weld(mesh, default_tolerance(&mesh.bounds()))
}

/// A fast hash for cell coordinates (FxHash's mixing); cells come from geometry, not from an
/// attacker, so the standard library's slow keyed hash buys nothing here.
#[derive(Default)]
struct CellHasher(u64);

impl Hasher for CellHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        // Arrays of integers arrive here as one run of bytes: take them eight at a time.
        let (words, rest) = bytes.as_chunks::<8>();
        for word in words {
            self.write_u64(u64::from_le_bytes(*word));
        }
        for &b in rest {
            self.write_u64(u64::from(b));
        }
    }

    fn write_i64(&mut self, v: i64) {
        self.write_u64(v as u64);
    }

    fn write_u64(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x517c_c1b7_2722_0a95);
    }

    fn write_usize(&mut self, v: usize) {
        self.write_u64(v as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes;

    #[test]
    fn a_soup_cube_becomes_eight_vertices() {
        let soup = shapes::soup(&shapes::cuboid([10.0, 10.0, 10.0]));
        let welded = weld_default(&soup);
        assert_eq!(welded.mesh.positions.len(), 8);
        assert_eq!(welded.mesh.triangles.len(), 12);
        assert_eq!(welded.source_triangle, (0..12).collect::<Vec<u32>>());
        assert_eq!(welded.vertex_map.len(), 36);
        assert_eq!(welded.collapsed, 0);
        assert!((welded.mesh.volume() - 1000.0).abs() < 1e-6);
    }

    #[test]
    fn nearly_equal_vertices_join_and_flat_triangles_go() {
        let mesh = Mesh::from_soup(&[
            [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.000_001, 0.0]],
            // Two corners at one place: this triangle has no area.
            [[1.0, 1.0, 0.0], [1.0, 1.0, 0.0], [2.0, 1.0, 0.0]],
        ]);
        let welded = weld(&mesh, 1e-4);
        assert_eq!(welded.mesh.positions.len(), 5);
        assert_eq!(welded.mesh.triangles, vec![[0, 1, 2], [1, 3, 2]]);
        assert_eq!(welded.source_triangle, vec![0, 1]);
        assert_eq!(welded.collapsed, 1);
        // The first vertex of a cell keeps its exact place.
        assert_eq!(welded.mesh.positions[2], [0.0, 1.0, 0.0]);
    }

    #[test]
    fn the_tolerance_follows_the_size_and_the_distance_from_the_origin() {
        let small = default_tolerance(&Bounds { min: [0.0; 3], max: [1.0; 3] });
        let large = default_tolerance(&Bounds { min: [0.0; 3], max: [200.0; 3] });
        assert!(small < large && large < 0.01);
        let far = default_tolerance(&Bounds { min: [100_000.0; 3], max: [100_001.0; 3] });
        assert!(far > 0.03, "f32 has steps of 0.008 out there: {far}");
        assert_eq!(default_tolerance(&Bounds::EMPTY), 1e-6);
    }
}
