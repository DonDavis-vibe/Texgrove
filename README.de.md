# Texgrove

**Texturen als echte Geometrie für 3D-Drucke.** Texgrove bringt Rändel, Geflechte, Holzmaserung
und andere Höhenmuster auf ein Modell und schreibt das Ergebnis als Netz, das jeder Slicer
annimmt. Es entsteht als Plugin für [Meshgrove](https://github.com/DonDavis-vibe/MeshGrove),
daneben gibt es eine Kommandozeile.

> **In Arbeit.** Der Kern (`texgrove-core`) und eine Kommandozeile bringen die eingebauten Muster
> schon auf ein Modell; das Reduzieren auf ein Dreiecksbudget, Masken, Bild-Texturen und das
> Plugin selbst kommen als Nächstes. Veröffentlicht ist noch nichts.

*English version: [README.md](README.md)*

## Was heute geht

```sh
cargo run --release -p texgrove-cli -- diag modell.stl             # geschlossen? ein Stück? Außenseite außen?
cargo run --release -p texgrove-cli -- convert modell.stl aus.3mf  # Punkte verbinden, STL oder 3MF schreiben
cargo run --release -p texgrove-cli -- apply modell.stl -o raendel.3mf --texture knurl --tile 6 --depth 0.8 --mapping cylindrical
```

## Entwicklung

```
crates/texgrove-core   Netz-Kern: Verschweißen, Kanten, Normalen, Diagnose, Strahlen-Baum, STL und 3MF
crates/texgrove-cli    der Befehl `texgrove`
scripts/check.sh       Format, Clippy und Tests, so wie CI sie fahren wird
tasks/todo.md          die Etappen und ihr Stand
```

## Dank

Die Idee stammt von [BumpMesh](https://bumpmesh.com) von Stefan Hermann (CNC Kitchen), das
dasselbe im Browser macht. Texgrove ist von Grund auf in Rust geschrieben und übernimmt weder
Code noch Texturen daraus.

## Lizenz

MIT oder Apache-2.0, nach Wahl. © 2026 Davis ([German Redneck Engineering](https://redneck-design.de/)).
