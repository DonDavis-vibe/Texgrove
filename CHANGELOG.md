# Changelog

Texgrove is a Meshgrove plugin: a release is one `texgrove.wasm`, installed under *Settings →
Plugins*. It needs Meshgrove 0.9.0 or newer. The command line and the kernel carry the same
version number.

## 1.0.1 – 2026-11-10

- A stamp on the side of a round part (a knob, a cup) took many times longer than on a flat
  face: rays running exactly along an axis walked half of the ray tree. They take the short
  way now; the preview of such a stamp comes in seconds again.

## 1.0.0 – 2026-11-10

The first release: textures as real geometry, on your models, inside Meshgrove.

- **From the gallery to the file**: right-click one or several models (or press the button in
  the Info pane) and choose "Apply texture …". The panel shows a preview, the textures as
  pictures, where the texture goes and how fine the file gets. Each textured file lands beside
  its model, never in its place, tagged "Textured" and with every setting in its note; undo is
  in Meshgrove's history.
- **Textures that ship**: ten patterns made by formulas (knurl, grooves, ribs, stripes, dots,
  hexagons, cells, noise, bricks, weave), twelve nature and material maps from Poly Haven
  (bark, wood, leather, rock, cobblestones, brick, slate, corrugated iron, gravel, rust, metal
  plate) and 32 patterns from Hero Patterns, in gallery groups, each with its credit.
- **Your own pictures**: a PNG or JPEG becomes a texture (bright is high), kept for next time,
  with smoothing for grainy photos, and favourites.
- **Laid on** from all round, like a box, round a cylinder, along the part's outline, round a
  sphere, flat from one side, or once as a stamp where you click; tile size and depth in
  millimetres, rotation, in and out or only one way.
- **Where the texture goes**: everywhere, outside only (bores and hollows keep their size, so
  fits still fit) or inside only; the bottom stays smooth so the part lies flat. By hand: a
  click marks a whole surface, a brush marks places, as "no texture" or "texture only here",
  with undo and redo; what gets no texture shows in orange.
- **Layers**: up to four textures on one model, each with its own settings and marks, covering
  or adding to those below.
- **Lay on a face**: a click turns a model so that face lies on the print bed.
- **Slicer projects**: a 3MF with several objects lists them; only the ticked ones get the
  texture, and Meshgrove swaps just those into a copy of the project, so plates, profiles and
  the other objects stay as they were.
- **Files a slicer likes**: the mesh is refined only where the texture goes, every vertex moves
  once along its normal so a closed model stays closed, and the result is reduced to a
  triangle budget (draft, normal, fine, from the texture's finest feature and the printer's
  nozzle). Output is 3MF or STL; the same input gives the same bytes on Linux and Windows.
- **Several at once**: the same settings on a whole selection, one step in the history.
- **For an AI**: tools to list the textures (with a numbered picture), to look at a model, to
  see a picture of a texture on it, and to make the file, which is made in the background and
  then waits in Meshgrove's review.
- **Command line**: `texgrove diag`, `texgrove convert`, `texgrove apply`, `texgrove pattern`,
  for the same work without Meshgrove.
- **German and English**, following Meshgrove's language.

The idea comes from [BumpMesh](https://bumpmesh.com) by Stefan Hermann (CNC Kitchen). Texgrove
is written from scratch and takes no code or textures from it.
