//! Texgrove's mesh kernel.
//!
//! Everything here is plain Rust without a platform underneath, so the command line and the
//! Meshgrove plugin (wasm32-wasip2) compute the same results. Lengths are millimetres.
//!
//! The way of a mesh through this crate:
//!
//! 1. [`weld`] joins the vertices of a triangle soup, so triangles know their neighbours.
//! 2. [`topology`] finds the edges, [`normals`] the directions a surface point moves along.
//! 3. [`diagnostics`] says whether the mesh is closed and in one piece.
//! 4. [`subdivide`] cuts the triangles small enough to carry a texture, [`project`] says where
//!    on a [`heightmap`] each vertex lies, and [`displace`] moves it; [`pipeline`] runs all of it.
//! 5. [`export`] writes binary STL and 3MF.
//!
//! [`bvh`] answers "what does this ray hit" and "which triangles are near this point" on
//! millions of triangles.

pub mod bvh;
pub mod diagnostics;
pub mod displace;
pub mod export;
pub mod hash;
pub mod heightmap;
pub mod import;
pub mod mesh;
pub mod normals;
pub mod pattern;
pub mod pipeline;
pub mod project;
pub mod shapes;
pub mod subdivide;
pub mod topology;
pub mod vec;
pub mod weld;

pub use mesh::{Bounds, Mesh, MeshError};
