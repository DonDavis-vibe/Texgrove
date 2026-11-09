//! Moving the vertices of a refined mesh by what the texture says.

use crate::heightmap::Heightmap;
use crate::mesh::Mesh;
use crate::project::Projector;
use crate::vec::{self, P3};

/// How heights become millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Relief {
    /// The way from the texture's lowest to its highest point, in mm.
    pub depth: f32,
    /// The height of the texture that leaves the surface where it is: 0.5 raises the bright
    /// half and sinks the dark half, 0 only raises, 1 only sinks.
    pub midlevel: f32,
    /// Swaps high and low.
    pub invert: bool,
}

impl Default for Relief {
    fn default() -> Self {
        Relief { depth: 0.5, midlevel: 0.5, invert: false }
    }
}

impl Relief {
    /// How far a point with texture height `h` (0 to 1) moves outwards, in mm.
    #[inline]
    pub fn offset(&self, h: f32) -> f32 {
        let h = if self.invert { 1.0 - h } else { h };
        self.depth * (h - self.midlevel.clamp(0.0, 1.0))
    }
}

/// Moves every vertex along its normal by the texture's height there.
///
/// `normals` holds one unit normal per vertex ([`crate::normals::vertex_normals`]). Each vertex
/// moves once, for all the triangles round it, so the mesh stays as closed as it was. `weights`
/// scales the way of each vertex (0 holds it in place); `None` moves all fully.
pub fn displace(
    mesh: &mut Mesh,
    normals: &[P3],
    weights: Option<&[f32]>,
    projector: &Projector,
    map: &Heightmap,
    relief: &Relief,
) {
    for (v, p) in mesh.positions.iter_mut().enumerate() {
        let weight = weights.map_or(1.0, |w| w[v]);
        if weight <= 0.0 {
            continue;
        }
        let normal = normals[v];
        let offset = relief.offset(projector.height(map, *p, normal)) * weight;
        *p = vec::add(*p, vec::scale(normal, offset));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Mapping, Placement};

    #[test]
    fn heights_become_millimetres() {
        let relief = Relief { depth: 2.0, midlevel: 0.5, invert: false };
        assert_eq!((relief.offset(0.0), relief.offset(0.5), relief.offset(1.0)), (-1.0, 0.0, 1.0));
        let raised = Relief { midlevel: 0.0, ..relief };
        assert_eq!((raised.offset(0.0), raised.offset(1.0)), (0.0, 2.0));
        let sunk = Relief { invert: true, ..raised };
        assert_eq!((sunk.offset(0.0), sunk.offset(1.0)), (2.0, 0.0));
    }

    #[test]
    fn vertices_move_along_their_normal_by_their_weight() {
        let mut mesh = Mesh { positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], triangles: vec![[0, 1, 2]] };
        let white = Heightmap::new(1, 1, vec![1.0]).unwrap();
        let projector = Projector::new(Mapping::PlanarXY, &Placement::default(), &mesh.bounds(), 1.0);
        let up = [[0.0, 0.0, 1.0]; 3];
        let relief = Relief { depth: 0.8, midlevel: 0.0, invert: false };
        displace(&mut mesh, &up, Some(&[1.0, 0.5, 0.0]), &projector, &white, &relief);
        assert_eq!(mesh.positions, vec![[0.0, 0.0, 0.8], [1.0, 0.0, 0.4], [0.0, 1.0, 0.0]]);
    }
}
