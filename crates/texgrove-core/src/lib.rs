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
//! 4. [`export`] writes binary STL and 3MF.
//!
//! [`bvh`] answers "what does this ray hit" and "which triangles are near this point" on
//! millions of triangles.

pub mod bvh;
pub mod diagnostics;
pub mod export;
pub mod import;
pub mod mesh;
pub mod normals;
pub mod shapes;
pub mod topology;
pub mod vec;
pub mod weld;

pub use mesh::{Bounds, Mesh, MeshError};
