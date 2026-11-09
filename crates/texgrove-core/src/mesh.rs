//! Triangles with shared vertices.

use crate::vec::{self, D3, P3};

/// A triangle mesh in millimetres: vertices, and three vertex numbers per triangle
/// (counter-clockwise seen from outside).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<P3>,
    pub triangles: Vec<[u32; 3]>,
}

/// Why data is not a mesh.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MeshError {
    #[error("positions and indices must come in threes")]
    NotInThrees,
    #[error("a coordinate is not a finite number")]
    NotFinite,
    #[error("triangle {triangle} points at vertex {vertex}, but there are only {vertices}")]
    IndexOutOfRange { triangle: usize, vertex: u32, vertices: usize },
    #[error("more vertices or triangles than fit ({0})")]
    TooLarge(usize),
}

/// The smallest and largest corner of a box round something.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: P3,
    pub max: P3,
}

impl Bounds {
    /// Nothing yet: growing it by a point makes it that point.
    pub const EMPTY: Bounds = Bounds { min: [f32::INFINITY; 3], max: [f32::NEG_INFINITY; 3] };

    pub fn grow(&mut self, p: P3) {
        self.min = vec::min(self.min, p);
        self.max = vec::max(self.max, p);
    }

    pub fn join(&mut self, other: &Bounds) {
        self.min = vec::min(self.min, other.min);
        self.max = vec::max(self.max, other.max);
    }

    pub fn is_empty(&self) -> bool {
        self.min[0] > self.max[0]
    }

    pub fn size(&self) -> P3 {
        if self.is_empty() { [0.0; 3] } else { vec::sub(self.max, self.min) }
    }

    pub fn centre(&self) -> P3 {
        vec::scale(vec::add(self.min, self.max), 0.5)
    }

    pub fn diagonal(&self) -> f32 {
        vec::length(self.size())
    }

    /// Half the surface of the box: what a bounding volume hierarchy weighs its nodes by.
    pub fn half_area(&self) -> f32 {
        let [x, y, z] = self.size();
        x * y + y * z + z * x
    }
}

impl Mesh {
    /// Every triangle with its own three vertices, as an STL has them. [`crate::weld::weld`]
    /// joins them.
    pub fn from_soup(triangles: &[[P3; 3]]) -> Mesh {
        let mut mesh =
            Mesh { positions: Vec::with_capacity(triangles.len() * 3), triangles: Vec::with_capacity(triangles.len()) };
        for (i, t) in triangles.iter().enumerate() {
            mesh.positions.extend_from_slice(t);
            let i = i as u32 * 3;
            mesh.triangles.push([i, i + 1, i + 2]);
        }
        mesh
    }

    /// From flat lists (three coordinates per vertex, three indices per triangle), as Meshgrove
    /// hands meshes to plugins. The data is checked.
    pub fn from_flat(positions: &[f32], indices: &[u32]) -> Result<Mesh, MeshError> {
        let (points, rest) = positions.as_chunks::<3>();
        let (triangles, rest_i) = indices.as_chunks::<3>();
        if !rest.is_empty() || !rest_i.is_empty() {
            return Err(MeshError::NotInThrees);
        }
        let mesh = Mesh { positions: points.to_vec(), triangles: triangles.to_vec() };
        mesh.check()?;
        Ok(mesh)
    }

    /// Flat lists of the mesh: coordinates and indices.
    pub fn to_flat(&self) -> (Vec<f32>, Vec<u32>) {
        (self.positions.as_flattened().to_vec(), self.triangles.as_flattened().to_vec())
    }

    /// Whether the mesh is whole: finite coordinates, indices in range, counts that fit `u32`.
    pub fn check(&self) -> Result<(), MeshError> {
        if self.positions.len() > u32::MAX as usize || self.triangles.len() > (u32::MAX / 3) as usize {
            return Err(MeshError::TooLarge(self.positions.len().max(self.triangles.len())));
        }
        if self.positions.as_flattened().iter().any(|c| !c.is_finite()) {
            return Err(MeshError::NotFinite);
        }
        let n = self.positions.len();
        for (i, t) in self.triangles.iter().enumerate() {
            if let Some(&vertex) = t.iter().find(|&&v| v as usize >= n) {
                return Err(MeshError::IndexOutOfRange { triangle: i, vertex, vertices: n });
            }
        }
        Ok(())
    }

    pub fn bounds(&self) -> Bounds {
        let mut bounds = Bounds::EMPTY;
        for &p in &self.positions {
            bounds.grow(p);
        }
        bounds
    }

    /// The three corners of triangle `t`.
    #[inline]
    pub fn corners(&self, t: usize) -> [P3; 3] {
        let [a, b, c] = self.triangles[t];
        [self.positions[a as usize], self.positions[b as usize], self.positions[c as usize]]
    }

    /// Twice the area of triangle `t` as a vector along its normal.
    #[inline]
    pub fn area_vector(&self, t: usize) -> D3 {
        let [a, b, c] = self.corners(t).map(vec::to64);
        vec::cross64(vec::sub64(b, a), vec::sub64(c, a))
    }

    /// The surface in mm².
    pub fn area(&self) -> f64 {
        (0..self.triangles.len()).map(|t| vec::length64(self.area_vector(t)) / 2.0).sum()
    }

    /// The enclosed volume in mm³: positive when the triangles face outwards, and only
    /// meaningful for a closed mesh.
    pub fn volume(&self) -> f64 {
        // Measured from the middle of the box, so far-off models lose no digits.
        let origin = if self.positions.is_empty() { [0.0; 3] } else { vec::to64(self.bounds().centre()) };
        let six: f64 = (0..self.triangles.len())
            .map(|t| {
                let [a, b, c] = self.corners(t).map(|p| vec::sub64(vec::to64(p), origin));
                vec::dot64(a, vec::cross64(b, c))
            })
            .sum();
        six / 6.0
    }

    /// Adds another mesh; its triangles keep pointing at its own vertices.
    pub fn append(&mut self, other: &Mesh) {
        let offset = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.triangles.extend(other.triangles.iter().map(|t| t.map(|v| v + offset)));
    }

    /// The mesh moved by `by`.
    pub fn translated(mut self, by: P3) -> Mesh {
        for p in &mut self.positions {
            *p = vec::add(*p, by);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes;

    #[test]
    fn a_cube_has_its_area_and_volume() {
        let cube = shapes::cuboid([10.0, 20.0, 30.0]);
        assert_eq!(cube.triangles.len(), 12);
        assert!((cube.area() - 2.0 * (200.0 + 600.0 + 300.0)).abs() < 1e-6);
        assert!((cube.volume() - 6000.0).abs() < 1e-6);
        assert_eq!(cube.bounds(), Bounds { min: [0.0; 3], max: [10.0, 20.0, 30.0] });
        // Far from the origin the volume stays exact.
        let far = cube.translated([5000.0, -7000.0, 900.0]);
        assert!((far.volume() - 6000.0).abs() < 1e-3);
    }

    #[test]
    fn flat_lists_are_checked() {
        assert_eq!(Mesh::from_flat(&[0.0, 0.0], &[]), Err(MeshError::NotInThrees));
        assert_eq!(Mesh::from_flat(&[0.0, 0.0, f32::NAN], &[]), Err(MeshError::NotFinite));
        let err = Mesh::from_flat(&[0.0; 9], &[0, 1, 3]).unwrap_err();
        assert_eq!(err, MeshError::IndexOutOfRange { triangle: 0, vertex: 3, vertices: 3 });
        let mesh = Mesh::from_flat(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], &[0, 1, 2]).unwrap();
        assert_eq!(mesh.to_flat().1, vec![0, 1, 2]);
    }

    #[test]
    fn empty_bounds_have_no_size() {
        let bounds = Mesh::default().bounds();
        assert!(bounds.is_empty());
        assert_eq!(bounds.size(), [0.0; 3]);
    }
}
