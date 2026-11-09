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

/// How a layer of texture meets what the layers below it made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Blend {
    /// Where this layer goes, it takes the place of those below.
    #[default]
    Cover,
    /// This layer's relief is added to those below.
    Add,
}

/// One layer of texture, set up for a model.
#[derive(Debug, Clone, Copy)]
pub struct Coat<'a> {
    pub projector: &'a Projector,
    pub map: &'a Heightmap,
    pub relief: &'a Relief,
    pub blend: Blend,
}

/// Works out how far each vertex moves for one layer of texture and blends it into `offsets`
/// (mm along the vertex's normal, one per vertex).
///
/// `normals` holds one unit normal per vertex ([`crate::normals::vertex_normals`]); `weights`
/// says how much of the layer each vertex takes (0 none; `None` for all of it everywhere).
pub fn lay_on(offsets: &mut [f32], mesh: &Mesh, normals: &[P3], weights: Option<&[f32]>, coat: &Coat) {
    for (v, offset) in offsets.iter_mut().enumerate() {
        let weight = weights.map_or(1.0, |w| w[v]);
        if weight <= 0.0 {
            continue;
        }
        let layer = coat.relief.offset(coat.projector.height(coat.map, mesh.positions[v], normals[v]));
        *offset = match coat.blend {
            Blend::Cover => *offset * (1.0 - weight) + layer * weight,
            Blend::Add => *offset + layer * weight,
        };
    }
}

/// Moves every vertex along its normal by its offset. Each vertex moves once, for all the
/// triangles round it, so the mesh stays as closed as it was.
pub fn shift(mesh: &mut Mesh, normals: &[P3], offsets: &[f32]) {
    for ((p, normal), offset) in mesh.positions.iter_mut().zip(normals).zip(offsets) {
        if *offset != 0.0 {
            *p = vec::add(*p, vec::scale(*normal, *offset));
        }
    }
}

/// One texture onto a mesh: [`lay_on`] and [`shift`] in one go.
pub fn displace(
    mesh: &mut Mesh,
    normals: &[P3],
    weights: Option<&[f32]>,
    projector: &Projector,
    map: &Heightmap,
    relief: &Relief,
) {
    let mut offsets = vec![0.0; mesh.positions.len()];
    lay_on(&mut offsets, mesh, normals, weights, &Coat { projector, map, relief, blend: Blend::Cover });
    shift(mesh, normals, &offsets);
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

    #[test]
    fn a_layer_covers_or_adds() {
        let mesh = Mesh { positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], triangles: vec![[0, 1, 2]] };
        let white = Heightmap::new(1, 1, vec![1.0]).unwrap();
        let projector = Projector::new(Mapping::PlanarXY, &Placement::default(), &mesh.bounds(), 1.0);
        let up = [[0.0, 0.0, 1.0]; 3];
        let relief = Relief { depth: 1.0, midlevel: 0.0, invert: false };
        let below = [0.4f32; 3];
        let weights = [1.0, 0.5, 0.0];
        let mut cover = below;
        let coat = Coat { projector: &projector, map: &white, relief: &relief, blend: Blend::Cover };
        lay_on(&mut cover, &mesh, &up, Some(&weights), &coat);
        assert_eq!(cover, [1.0, 0.7, 0.4]);
        let mut add = below;
        lay_on(&mut add, &mesh, &up, Some(&weights), &Coat { blend: Blend::Add, ..coat });
        assert_eq!(add, [1.4, 0.9, 0.4]);
    }
}
