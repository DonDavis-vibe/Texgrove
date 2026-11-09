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
- [x] `heightmap` (bilinear, wiederholt/gespiegelt, Weichzeichnen, Graustufenbild rein/raus)
- [x] `pattern` (Rändel, Rillen, Rippen, Streifen, Punkte, Waben, Voronoi, Rauschen, Ziegel, Geflecht;
      jedes kennt sein feinstes Detail → Standard-Kantenlänge)
- [x] `project` (Triplanar, Box, Zylinder mit ganzer Kachelzahl + Deckel, Kugel, Planar XY/XZ/YZ;
      mm-Maße, Versatz, Drehung, von außen nie gespiegelt)
- [x] `subdivide` (Kanten halbieren bis ≤ L, `parent` je Dreieck, Teilauswahl ohne Spalt,
      Schätzung zählt exakt „in Gedanken“, Kantenlänge fürs Budget)
- [x] `displace` (ein Normal je Punkt, Tiefe/Mittelwert/Invertieren, Gewichte)
- [x] `pipeline::texture` mit Kantenschutz (scharfe Kanten bleiben stehen)
- [x] CLI `apply`, `pattern` (PGM)
- [x] Erste Ergebnisse: Adapter mit Rändel/Waben/Geflecht, Würfel mit Ziegeln/Voronoi
      (`out/first-textures.png`); 8,3 M Dreiecke in 5 s, 590 MB
- [x] `decimate` (QEM: Ränder halten ihre Linie, Flächen wachsen nicht zusammen, kein Umklappen,
      Finnen bleiben; Budget + Toleranz; Fortschritt/Abbruch; Schubladen-Warteschlange statt Heap)
- [x] In `pipeline::texture_with` (Stufen Verfeinern → Verschieben → Reduzieren, abbrechbar) und
      CLI (`--triangles`, `--tolerance`)
- [x] Gemessen: Adapter 8,3 M → 1 M in 24 s (1,2 GB), Volumen ±0,004 %; Ziegel-Würfel 786 k → 8,7 k
      in 1,5 s (`out/reduced.png`)
- [ ] Tempo beim Reduzieren großer Netze (≈ 5 µs je Kante, Speicherzugriffe quer durchs Netz):
      in M10 messen, ggf. räumlich geordnet arbeiten; und nicht 8× mehr verfeinern als am Ende bleibt
- [ ] `mask` (Flags, Tupfer, Winkel, Randauslauf), `layers`, `quality` (Stufen aus Düse)
- [ ] Flip-Nachlauf, Überlappungswarnung
- [ ] Outline wrap, Stempel (M8), Naht-Blend für Box/Zylinder
- [ ] Eigenschaftstests (proptest), `bench`

Aufgefallen: Kantenlänge wirkt in Stufen (halbiert oder nicht) – ein Dreiecksbudget lässt sich
über die Kantenlänge nur grob treffen; das Budget fürs Ergebnis macht das Reduzieren.

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
