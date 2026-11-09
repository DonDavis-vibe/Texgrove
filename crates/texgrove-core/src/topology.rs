//! Which triangles meet at which edge and vertex.
//!
//! Works on welded meshes ([`crate::weld`]). A triangle has three *sides*: side `k` runs from its
//! corner `k` to corner `k + 1`. Sides of different triangles between the same two vertices form
//! an *edge*.

use crate::mesh::Mesh;

/// How many triangles an edge has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    /// One triangle: the rim of an open surface.
    Boundary,
    /// Two triangles, as everywhere on a closed surface.
    Manifold,
    /// Three or more: surfaces grown together along a line.
    NonManifold,
}

/// One side of a triangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Side {
    pub triangle: u32,
    /// 0, 1 or 2: the side from this corner to the next.
    pub corner: u8,
}

impl Side {
    #[inline]
    fn from_number(n: u32) -> Side {
        Side { triangle: n / 3, corner: (n % 3) as u8 }
    }

    #[inline]
    fn number(self) -> u32 {
        self.triangle * 3 + u32::from(self.corner)
    }

    /// The vertices this side runs from and to.
    #[inline]
    pub fn vertices(self, mesh: &Mesh) -> [u32; 2] {
        let t = mesh.triangles[self.triangle as usize];
        [t[self.corner as usize], t[(self.corner as usize + 1) % 3]]
    }
}

/// The edges of a mesh.
///
/// Memory: 12 bytes per triangle side plus 4 per edge, so about 300 MB for ten million triangles.
#[derive(Debug, Clone, Default)]
pub struct Topology {
    /// All sides, those of one edge next to each other.
    sides: Vec<u32>,
    /// Where each edge's sides start in `sides`; one more entry than edges.
    edge_start: Vec<u32>,
    /// The edge of every side.
    side_edge: Vec<u32>,
}

impl Topology {
    pub fn build(mesh: &Mesh) -> Topology {
        let key = |side: u32| {
            let [a, b] = Side::from_number(side).vertices(mesh);
            (u64::from(a.min(b)) << 32) | u64::from(a.max(b))
        };
        let count = mesh.triangles.len() as u32 * 3;
        let mut sides: Vec<u32> = (0..count).collect();
        // Sorting the numbers by their key needs no second array of keys; equal keys keep the
        // order of the triangles, so the result does not depend on the sort's inner workings.
        sides.sort_unstable_by_key(|&s| (key(s), s));
        let mut edge_start = Vec::with_capacity(count as usize / 2 + 1);
        let mut side_edge = vec![0u32; count as usize];
        let mut previous = None;
        for (i, &side) in sides.iter().enumerate() {
            let k = key(side);
            if previous != Some(k) {
                edge_start.push(i as u32);
                previous = Some(k);
            }
            side_edge[side as usize] = edge_start.len() as u32 - 1;
        }
        edge_start.push(count);
        Topology { sides, edge_start, side_edge }
    }

    pub fn edge_count(&self) -> usize {
        self.edge_start.len().saturating_sub(1)
    }

    /// The edge a side belongs to.
    #[inline]
    pub fn edge_of(&self, triangle: u32, corner: u8) -> u32 {
        self.side_edge[Side { triangle, corner }.number() as usize]
    }

    /// How many triangles meet at the edge.
    #[inline]
    pub fn triangle_count(&self, edge: u32) -> usize {
        (self.edge_start[edge as usize + 1] - self.edge_start[edge as usize]) as usize
    }

    /// The sides that form the edge, in the order of their triangles.
    pub fn edge_sides(&self, edge: u32) -> impl ExactSizeIterator<Item = Side> + '_ {
        let range = self.edge_start[edge as usize] as usize..self.edge_start[edge as usize + 1] as usize;
        self.sides[range].iter().map(|&s| Side::from_number(s))
    }

    /// The two vertices of an edge, the smaller number first.
    pub fn edge_vertices(&self, mesh: &Mesh, edge: u32) -> [u32; 2] {
        let [a, b] = Side::from_number(self.sides[self.edge_start[edge as usize] as usize]).vertices(mesh);
        [a.min(b), a.max(b)]
    }

    #[inline]
    pub fn kind(&self, edge: u32) -> EdgeKind {
        match self.triangle_count(edge) {
            1 => EdgeKind::Boundary,
            2 => EdgeKind::Manifold,
            _ => EdgeKind::NonManifold,
        }
    }

    /// The side on the other side of a manifold edge; `None` at a rim or where more than two
    /// triangles meet.
    #[inline]
    pub fn opposite(&self, triangle: u32, corner: u8) -> Option<Side> {
        let edge = self.edge_of(triangle, corner) as usize;
        let (start, end) = (self.edge_start[edge] as usize, self.edge_start[edge + 1] as usize);
        if end - start != 2 {
            return None;
        }
        let me = Side { triangle, corner }.number();
        let other = if self.sides[start] == me { self.sides[start + 1] } else { self.sides[start] };
        Some(Side::from_number(other))
    }

    /// Whether the two triangles of a manifold edge agree on what is outside: they then run along
    /// the edge in opposite directions. `true` for other kinds of edge.
    pub fn agrees(&self, mesh: &Mesh, edge: u32) -> bool {
        let mut sides = self.edge_sides(edge);
        match (sides.next(), sides.next(), sides.next()) {
            (Some(a), Some(b), None) => a.vertices(mesh)[0] != b.vertices(mesh)[0],
            _ => true,
        }
    }

    /// Counts of each kind of edge, and of manifold edges whose triangles disagree.
    pub fn census(&self, mesh: &Mesh) -> Census {
        let mut census = Census::default();
        for edge in 0..self.edge_count() as u32 {
            match self.kind(edge) {
                EdgeKind::Boundary => census.boundary += 1,
                EdgeKind::NonManifold => census.non_manifold += 1,
                EdgeKind::Manifold if !self.agrees(mesh, edge) => census.disagreeing += 1,
                EdgeKind::Manifold => {}
            }
        }
        census.edges = self.edge_count();
        census
    }
}

/// How many edges of each kind a mesh has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Census {
    pub edges: usize,
    pub boundary: usize,
    pub non_manifold: usize,
    /// Manifold edges whose two triangles face opposite ways.
    pub disagreeing: usize,
}

/// The triangles round every vertex.
#[derive(Debug, Clone, Default)]
pub struct VertexTriangles {
    start: Vec<u32>,
    triangles: Vec<u32>,
}

impl VertexTriangles {
    pub fn build(mesh: &Mesh) -> VertexTriangles {
        let mut start = vec![0u32; mesh.positions.len() + 1];
        for t in &mesh.triangles {
            for &v in t {
                start[v as usize + 1] += 1;
            }
        }
        for v in 0..mesh.positions.len() {
            start[v + 1] += start[v];
        }
        let mut next = start.clone();
        let mut triangles = vec![0u32; mesh.triangles.len() * 3];
        for (i, t) in mesh.triangles.iter().enumerate() {
            for &v in t {
                triangles[next[v as usize] as usize] = i as u32;
                next[v as usize] += 1;
            }
        }
        VertexTriangles { start, triangles }
    }

    /// The triangles that have `vertex` as a corner, in rising order.
    #[inline]
    pub fn of(&self, vertex: u32) -> &[u32] {
        &self.triangles[self.start[vertex as usize] as usize..self.start[vertex as usize + 1] as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes;

    #[test]
    fn a_cube_is_closed_and_agrees_with_itself() {
        let cube = shapes::cuboid([1.0; 3]);
        let topology = Topology::build(&cube);
        assert_eq!(topology.census(&cube), Census { edges: 18, boundary: 0, non_manifold: 0, disagreeing: 0 });
        for t in 0..12 {
            for corner in 0..3 {
                let other = topology.opposite(t, corner).unwrap();
                assert_ne!(other.triangle, t);
                assert_eq!(topology.opposite(other.triangle, other.corner), Some(Side { triangle: t, corner }));
                let [a, b] = Side { triangle: t, corner }.vertices(&cube);
                assert_eq!(other.vertices(&cube), [b, a]);
                assert_eq!(topology.edge_vertices(&cube, topology.edge_of(t, corner)), [a.min(b), a.max(b)]);
            }
        }
    }

    #[test]
    fn a_sheet_has_a_rim() {
        let sheet = shapes::sheet([3.0, 2.0], [3, 2]);
        let topology = Topology::build(&sheet);
        let census = topology.census(&sheet);
        assert_eq!((census.boundary, census.non_manifold, census.disagreeing), (10, 0, 0));
        assert_eq!(census.edges, 10 + 3 + 4 + 6); // rim, inner lines along x and y, diagonals
        let rim = (0..census.edges as u32).find(|&e| topology.kind(e) == EdgeKind::Boundary).unwrap();
        let side = topology.edge_sides(rim).next().unwrap();
        assert_eq!(topology.opposite(side.triangle, side.corner), None);
    }

    #[test]
    fn a_turned_triangle_and_a_fin_are_found() {
        let mut cube = shapes::cuboid([1.0; 3]);
        cube.triangles[0].swap(1, 2);
        let census = Topology::build(&cube).census(&cube);
        assert_eq!((census.boundary, census.non_manifold, census.disagreeing), (0, 0, 3));

        // A third triangle on an edge of the cube.
        let mut cube = shapes::cuboid([1.0; 3]);
        cube.positions.push([0.5, -1.0, -1.0]);
        cube.triangles.push([0, 1, 8]);
        let topology = Topology::build(&cube);
        let census = topology.census(&cube);
        assert_eq!((census.boundary, census.non_manifold), (2, 1));
        let fin = (0..census.edges as u32).find(|&e| topology.kind(e) == EdgeKind::NonManifold).unwrap();
        assert_eq!(topology.edge_sides(fin).len(), 3);
        assert_eq!(topology.edge_vertices(&cube, fin), [0, 1]);
    }

    #[test]
    fn every_vertex_knows_its_triangles() {
        let cube = shapes::cuboid([1.0; 3]);
        let round = VertexTriangles::build(&cube);
        let total: usize = (0..8).map(|v| round.of(v).len()).sum();
        assert_eq!(total, 36);
        for v in 0..8u32 {
            assert!(round.of(v).iter().all(|&t| cube.triangles[t as usize].contains(&v)));
            assert!(round.of(v).is_sorted());
        }
    }
}
