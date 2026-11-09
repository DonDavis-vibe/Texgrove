# Texgrove – Etappen

Plan: `/home/davis/.claude/plans/ich-antworte-einzeln-shiny-kitten.md` (freigegeben 09.11.2026).
Ein Release am Ende (Texgrove 1.0.0 mit Meshgrove 0.9.0); die Etappen sind intern.

## M0 Einrichten
- [x] BumpMesh-Klon gelöscht
- [x] Repo nach Bingroves Muster (Workspace, Lizenzen, check.sh, README de/en, CHANGELOG)
- [x] Host-Arbeit läuft in eigener Session/eigenem Worktree (Plugin-API 0.2 Teil 1)
- [x] Erster Commit; GitHub-Repo DonDavis-vibe/Texgrove, **privat** bis Davis es anders sagt (09.11.)

## M1 Geometrie-Kern
- [x] `mesh`, `vec`, `shapes`
- [x] `weld` (Zellen-Hash, Toleranz aus Modellgröße)
- [x] `topology` (Kanten, Gegenseite, Rand/Finne, Einigkeit; Dreiecke je Punkt)
- [x] `normals` (Flächen, Punkte nach Winkel/Fläche, scharfe Kanten)
- [x] `diagnostics` (offen, Finnen, Schalen, flach, Volumen)
- [x] `bvh` (Strahl, verdeckt, Umkreis, nächster Punkt; gegen Durchprobieren getestet)
- [x] Schreiber STL + 3MF (deterministisch), Leser STL
- [x] CLI `diag`, `convert`
- [x] Rauchtest mit echten Modellen (Adapter 19 k, Lithophanie 3 M Dreiecke in 4 s);
      Meshgroves Thumbnailer rendert STL und 3MF pixelgleich
- [ ] Davis: `adaptek.3mf` in Anycubic Slicer Next laden

Aufgefallen: `bambi/adaptek.stl` hat 192 Kanten mit mehr als zwei Dreiecken – echte Modelle sind
nicht immer sauber; Unterteilen/Verschieben/Reduzieren (M2) müssen damit umgehen.

## M2 Textur-Pipeline
- [ ] `heightmap` (bilinear, Kacheln, Weichzeichnen)
- [ ] `textures/procedural` (Rändel, Waben, Voronoi, Rauschen, Streifen, Punkte, Ziegel, Geflecht, Rillen)
- [ ] `project` (Triplanar, Box, Zylinder, Kugel, Planar; später Outline wrap, Stempel)
- [ ] `subdivide` (kantenlängen-adaptiv, `parent` je Dreieck)
- [ ] `mask` (Flags, Tupfer, Winkel, Randauslauf)
- [ ] `displace` (ein Normal je Punkt, Kantenschutz, Flip-Nachlauf)
- [ ] `decimate` (QEM mit Schutzregeln)
- [ ] `quality`, `layers`, `pipeline`
- [ ] CLI `apply`, `bench`; Eigenschaftstests; Benchmarks

## M3 Plugin-API 0.2 Teil 1 (Host, eigene Session)
## M4 Plugin-Grundstock ohne Malen
## M5 Textur-Bestand (CC0, Hero Patterns, eigene)
## M6 Plugin-API 0.2 Teil 2 (Malen, Overlay, Performance)
## M7 Masken und Ebenen
## M8 Erweiterte Projektionen
## M9 KI-Werkzeuge
## M10 Fertigstellen
- [ ] Grafiken (Krea 2 Turbo über ComfyUI), README, CHANGELOG, CI/Release
- [ ] **Test unter Linux ganz normal lokal** (Davis, 09.11.)
- [ ] **Test in der Windows-VM** (Davis, 09.11.: „vor allem auch“)
- [ ] Info an CNC Kitchen (Text entwerfen, Davis schickt)
