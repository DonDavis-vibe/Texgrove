# Changelog

What changed in Texgrove, newest first. Versions follow the plugin; the command line and the
kernel carry the same number.

## Unreleased

The mesh kernel, and the first Meshgrove panel.

- **Plugin**: right-click models in Meshgrove's gallery and choose "Apply texture …"; the panel
  shows a preview, offers the patterns as pictures, where the texture goes (everywhere, outside
  only, inside only, bottom left smooth) and the quality, and puts each textured file beside its
  model, tagged "Textured" and with every setting in its note.
- **Slicer projects**: a 3MF with several objects lists them; only the ticked ones get the
  texture, and Meshgrove swaps just those into a copy of the project, so plates, profiles and the
  other objects stay as they were.
- **Marks by hand**: tools below the preview mark whole surfaces with a click or places with a
  brush, as "no texture" or "texture only here", with undo and redo; what gets no texture shows
  in orange. (Needs Meshgrove 0.9.0's painting in the 3D view.)
- **Lay on a face**: a click turns a model so that face lies on the print bed.
- **Layers**: up to four textures on one model, each with its own settings and marks, covering
  or adding to those below; a new layer starts empty and goes where you mark.
- **Stamp, outline, soft seams**: a texture once where you click; wrapped along the part's
  outline; box and cylinder blend at their edges.
- **For an AI**: tools to list the textures (with a numbered picture), to look at a model, to
  see a picture of a texture on it, and to make the file, which then waits in Meshgrove's
  review. (Making files needs Meshgrove 0.9.0's background jobs for tools.)
- **Textures that ship**: twelve nature and material maps from Poly Haven (bark, wood, leather,
  rock, cobblestones, brick, slate, corrugated iron, gravel, rust, metal plate) and 32 patterns
  from Hero Patterns, in gallery groups, each with its credit.
- **Your own pictures**: a PNG or JPEG becomes a texture (bright is high), kept for next time,
  with smoothing for grainy photos.

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
- **Reducing**: the textured mesh is brought down to a triangle budget by pulling together the
  edges that change the shape least; rims, holes and sharp edges stay, and flat stretches go
  back to a few triangles by themselves.
- **Where the texture goes**: leave out what looks down (so a part still lies flat) or up, pick
  faces, brush places in or out with a soft or hard rim, let the texture run out towards what
  stays. A part's inside (a bore, the inside of a cup) is found by itself, so only the outside
  or only the inside gets texture; a click picks a whole smooth surface up to its sharp edges.
- **Layers**: several textures on one model, each with its own mask, covering or adding to those
  below.
- **Quality**: draft, normal and fine choose the mesh's fineness and size from the texture's
  finest feature and the printer's nozzle.
- **Command line**: `texgrove diag`, `texgrove convert`, `texgrove apply`, `texgrove pattern`.
