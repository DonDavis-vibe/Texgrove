# Texgrove

**Textures as real geometry for 3D prints.** Texgrove puts knurls, weaves, wood grain and other
height patterns onto a model and writes the result as a mesh any slicer takes. It is being built
as a plugin for [Meshgrove](https://github.com/DonDavis-vibe/MeshGrove), with a command line
beside it.

> **Work in progress.** The kernel (`texgrove-core`) and a command line can already put the
> built-in patterns onto a model; reducing the result to a triangle budget, masks, picture
> textures and the plugin itself are next. Nothing is released yet.

*Deutsche Fassung: [README.de.md](README.de.md)*

## What works today

```sh
cargo run --release -p texgrove-cli -- diag model.stl             # closed? one piece? facing outwards?
cargo run --release -p texgrove-cli -- convert model.stl out.3mf  # join vertices, write STL or 3MF
cargo run --release -p texgrove-cli -- apply model.stl -o knurled.3mf --texture knurl --tile 6 --depth 0.8 --mapping cylindrical --where outside
```

## Development

```
crates/texgrove-core   mesh kernel: welding, edges, normals, diagnostics, ray tree, STL and 3MF
crates/texgrove-cli    the `texgrove` command
scripts/check.sh       format, clippy and tests, as CI will run them
tasks/todo.md          the milestones and where they stand
```

## Credits

The idea comes from [BumpMesh](https://bumpmesh.com) by Stefan Hermann (CNC Kitchen), which does
this in the browser. Texgrove is written from scratch in Rust and takes no code or textures
from it.

## License

MIT or Apache-2.0, at your choice. © 2026 Davis ([German Redneck Engineering](https://redneck-design.de/)).
