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
- **Textures**: ten built-in patterns made by formulas (knurl, grooves, ribs, stripes, dots,
  hexagons, voronoi, noise, bricks, weave), height maps with blur and mirrored tiling.
- **Laying on**: triplanar, box, cylinder (a whole number of tiles round, flat caps), globe and
  flat from each axis; tile size in millimetres, shift and rotation.
- **Texturing**: edges are halved until the mesh is fine enough, every vertex moves once along its
  normal, so a closed model stays closed; sharp edges can be held in place.
- **Command line**: `texgrove diag`, `texgrove convert`, `texgrove apply`, `texgrove pattern`.
