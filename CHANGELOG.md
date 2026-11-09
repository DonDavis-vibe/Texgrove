# Changelog

What changed in Texgrove, newest first. Versions follow the plugin; the command line and the
kernel carry the same number.

## Unreleased

The mesh kernel.

- **Welding**: the vertices of a triangle soup (an STL) become shared vertices.
- **Edges and normals**: which triangles meet where, sharp edges, one normal per vertex that does
  not depend on how a face is cut into triangles.
- **Diagnostics**: open edges, fins, pieces, triangles facing the wrong way, volume.
- **Ray tree**: what a ray hits and which triangles lie near a point, on millions of triangles.
- **Files**: reads STL (binary and text), writes binary STL and 3MF with one named object per part;
  the same input gives the same bytes.
- **Command line**: `texgrove diag` and `texgrove convert`.
