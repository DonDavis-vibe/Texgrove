//! Is this mesh fit for printing: closed, in one piece, facing outwards?

use crate::mesh::{Bounds, Mesh};
use crate::topology::Topology;
use crate::vec;

/// What [`diagnose`] found.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub vertices: usize,
    pub triangles: usize,
    pub edges: usize,
    /// Edges with one triangle: holes in the surface.
    pub open_edges: usize,
    /// Edges with three or more triangles.
    pub non_manifold_edges: usize,
    /// Edges whose two triangles face opposite ways.
    pub disagreeing_edges: usize,
    /// Triangles without area (their corners lie on a line).
    pub flat_triangles: usize,
    /// Pieces not connected to each other by an edge.
    pub shells: usize,
    /// The surface in mm².
    pub area: f64,
    /// The enclosed volume in mm³; negative when the triangles face inwards. Only meaningful for
    /// a closed mesh.
    pub volume: f64,
    pub bounds: Bounds,
}

impl Report {
    /// No holes and no fins: every edge has exactly two triangles.
    pub fn closed(&self) -> bool {
        self.triangles > 0 && self.open_edges == 0 && self.non_manifold_edges == 0
    }

    /// Closed, with all triangles agreeing on the outside and facing it.
    pub fn sound(&self) -> bool {
        self.closed() && self.disagreeing_edges == 0 && self.volume > 0.0
    }

    /// Vertices − edges + faces: 2 per closed shell without a hole through it, 0 for a ring.
    pub fn euler(&self) -> i64 {
        self.vertices as i64 - self.edges as i64 + self.triangles as i64
    }
}

/// Examines a welded mesh ([`crate::weld`]); on a triangle soup every edge counts as open.
pub fn diagnose(mesh: &Mesh) -> Report {
    diagnose_with(mesh, &Topology::build(mesh))
}

/// [`diagnose`] with edges the caller has already found.
pub fn diagnose_with(mesh: &Mesh, topology: &Topology) -> Report {
    let census = topology.census(mesh);
    Report {
        vertices: mesh.positions.len(),
        triangles: mesh.triangles.len(),
        edges: census.edges,
        open_edges: census.boundary,
        non_manifold_edges: census.non_manifold,
        disagreeing_edges: census.disagreeing,
        flat_triangles: (0..mesh.triangles.len()).filter(|&t| is_flat(mesh, t)).count(),
        shells: shells(mesh, topology),
        area: mesh.area(),
        volume: mesh.volume(),
        bounds: mesh.bounds(),
    }
}

/// Whether a triangle has (as good as) no area: its height is below a billionth of its longest side.
pub fn is_flat(mesh: &Mesh, t: usize) -> bool {
    let [a, b, c] = mesh.corners(t).map(vec::to64);
    let longest =
        [vec::sub64(b, a), vec::sub64(c, b), vec::sub64(a, c)].iter().map(|e| vec::dot64(*e, *e)).fold(0.0, f64::max);
    vec::length64(mesh.area_vector(t)) <= 1e-9 * longest
}

/// The number of pieces: triangles belong together when they share an edge.
fn shells(mesh: &Mesh, topology: &Topology) -> usize {
    let mut parent: Vec<u32> = (0..mesh.triangles.len() as u32).collect();
    fn root(parent: &mut [u32], mut i: u32) -> u32 {
        while parent[i as usize] != i {
            parent[i as usize] = parent[parent[i as usize] as usize]; // halve the path
            i = parent[i as usize];
        }
        i
    }
    for edge in 0..topology.edge_count() as u32 {
        let mut sides = topology.edge_sides(edge);
        let first = root(&mut parent, sides.next().map_or(0, |s| s.triangle));
        for side in sides {
            let other = root(&mut parent, side.triangle);
            parent[other as usize] = first;
        }
    }
    (0..parent.len() as u32).filter(|&i| parent[i as usize] == i).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{shapes, weld};

    #[test]
    fn a_cube_is_sound() {
        let report = diagnose(&shapes::cuboid([10.0; 3]));
        assert!(report.sound());
        assert_eq!((report.shells, report.euler(), report.flat_triangles), (1, 2, 0));
        assert!((report.volume - 1000.0).abs() < 1e-6);
    }

    #[test]
    fn a_torus_has_a_hole_through_it_and_two_cubes_are_two_shells() {
        let ring = diagnose(&shapes::torus(20.0, 5.0, 24, 12));
        assert!(ring.sound());
        assert_eq!((ring.shells, ring.euler()), (1, 0));

        let mut two = shapes::cuboid([1.0; 3]);
        two.append(&shapes::cuboid([1.0; 3]).translated([5.0, 0.0, 0.0]));
        let report = diagnose(&two);
        assert!(report.sound());
        assert_eq!((report.shells, report.euler()), (2, 4));
    }

    #[test]
    fn holes_inside_out_and_soups_are_noticed() {
        let mut open = shapes::cuboid([1.0; 3]);
        open.triangles.truncate(10); // without its back
        let report = diagnose(&open);
        assert!(!report.closed());
        assert_eq!(report.open_edges, 4);

        let mut inside_out = shapes::cuboid([1.0; 3]);
        for t in &mut inside_out.triangles {
            t.swap(1, 2);
        }
        let report = diagnose(&inside_out);
        assert!(report.closed() && !report.sound());
        assert!(report.volume < 0.0);

        let soup = shapes::soup(&shapes::cuboid([1.0; 3]));
        assert_eq!(diagnose(&soup).open_edges, 36);
        assert!(diagnose(&weld::weld_default(&soup).mesh).sound());
        assert!(!diagnose(&Mesh::default()).closed());
    }

    #[test]
    fn a_triangle_on_a_line_is_flat() {
        let mesh = Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            triangles: vec![[0, 1, 2], [0, 1, 3]],
        };
        assert_eq!(diagnose(&mesh).flat_triangles, 1);
    }
}
