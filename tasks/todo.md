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
- [x] `mask`: Winkel oben/unten, Flächen aus-/einschließen, Pinsel-Tupfer (Radius, Härte,
      radieren, nur zugewandte Seite), weicher Auslauf; was stehen bleibt, bleibt bis zum Rand plan
- [x] `select`: „innen/außen“ automatisch (Wand blickt über einen Hohlraum auf eine Wand; je glatte
      Fläche als Ganzes entschieden), Klick wählt glatte Fläche bis zur scharfen Kante, Offenheit
- [x] Ebenen (`texture_layers`: verdecken/addieren, je Ebene eigene Maske), `Finish` getrennt
- [x] `quality` (Entwurf/Normal/Fein aus Musterdetail und Düse), `Heightmap::detail`
- [x] CLI: `--where all|outside|inside`, `--bottom-angle` (Standard 5°), `--top-angle`,
      `--falloff`, `--quality`, `--nozzle`
- [x] Davis' Slicer-Test (Anycubic Slicer Next, 09.11.): Ziegel-Würfel lädt und slict sauber
- [x] Am Adapter geprüft: nur außen Rändel / nur innen Rippen (`out/where.png`)
- [ ] Flip-Nachlauf, Überlappungswarnung
- [ ] Outline wrap, Stempel (M8), Naht-Blend für Box/Zylinder
- [ ] Eigenschaftstests (proptest), `bench`

**Davis, 09.11.: Man muss einstellen können, wo die Textur hinkommt** – Teile mit Innen- und
Außenseite dürfen nicht zwangsläufig überall texturiert werden, sonst leidet die Funktion
(Passungen, Bohrungen). Im Plugin deshalb gut sichtbar, nicht versteckt: „Überall / Nur außen /
Nur innen“ als Schnellwahl, dazu Fläche anklicken (füllen) und Pinsel; Unterseite standardmäßig
glatt. Rillen außen zählen bei der Automatik als „innen“ – das korrigiert man per Klick.

Aufgefallen: Kantenlänge wirkt in Stufen (halbiert oder nicht) – ein Dreiecksbudget lässt sich
über die Kantenlänge nur grob treffen; das Budget fürs Ergebnis macht das Reduzieren.

## M3 Plugin-API 0.2 Teil 1 (Host, eigene Session)
- [x] Fertig auf Branch `claude/happy-mahavira-d8760c` im MeshGrove-Repo (12 Commits, 09.11.,
      nicht gepusht, nicht in main); Tests von mir nachgeprüft (285 grün)
- [ ] Oberfläche des Branches in einer echten Sitzung prüfen (Mehrfach-Rechtsklick, Dialoge, Review)
- [ ] Teiletausch-3MF im Anycubic Slicer laden (Davis)
- [ ] Merge nach main und 0.9.0 mit der main-Session abstimmen (Davis entscheidet)

## M4 Plugin-Grundstock ohne Malen
- [x] Crate `texgrove-plugin` (eigener Workspace, `.cargo/config.toml` patcht die API lokal auf
      den Worktree; `scripts/build.sh` mit simd128) – 1,2 MB wasm
- [x] Manifest (API 0.2, `receive: ids` für stl/3mf/obj/ply/step/stp/glb/gltf, 3584 MB), de/en
- [x] Panel: Modelle (Rechtsklick, Dateidialog nach `/data/picked`, Suche), Galerie mit
      Vorschaubildern aus den Mustern, Auftragen/Kachel/Tiefe/Richtung/Drehung/Invertieren,
      **Wohin: Überall / Nur außen / Nur innen** + Unterseite/Oberseite frei, Qualität, Format
- [x] Vorschau-Job (grob, ≤ 600 k Dreiecke, ohne Reduzieren) mit Cache in `/data/preview`
- [x] Export-Jobs nacheinander, ein Bündel je Bibliothek (`next_to`, Tag „Texturiert“, Notiz mit
      allen Einstellungen); Dateien von außerhalb → Ordner „Texgrove“ im Import
- [x] `meshgrove-plugin-run`: Vorschau Würfel 0,4 s; Export Würfel 2,3 s, Adapter (nur außen,
      4,1 M → 1 M) 27 s im wasm
- [x] Rundlauf in isolierter Meshgrove-Sitzung (Debug-Build des API-0.2-Branches, eigene XDG-Ordner,
      „Bibliothek lesen“ = nein): Rechtsklick → „Textur auftragen …“ → Panel mit Vorschau →
      Ziegel → „Textur auftragen“ → `wuerfel – Ziegel.3mf` neben dem Original, Tag „Texturiert“,
      Notiz mit allen Einstellungen, Undo-Eintrag; Meshgroves Prüfung: geschlossen, 1 Teil
      (`out/panel.png`, `out/panel-ergebnis.png`)
- [x] Mehrteilige 3MF: Teile per Häkchen, Vorschau setzt jedes Teil an seinen Platz, Export
      texturiert die gewählten Teile in eigenen Koordinaten (Dreiecksbudget nach Fläche geteilt) →
      `derive_3mf`; geprüft an Davis' Bambu-Projekt mit 7 Teilen: nur das gewählte Objekt geändert,
      Einstellungen/Platten byte-gleich, Plattenbilder geleert (`out/helmhalter_teil_getauscht.3mf`)
- [ ] Davis: `out/helmhalter_teil_getauscht.3mf` im Anycubic Slicer laden
- [ ] Fortschritt der Vorschau beim Schieben von Reglern beobachten (Jobs abbrechen/neu)
## M5 Textur-Bestand (CC0, Hero Patterns, eigene)
- [x] Kern: PNG/JPEG → Höhenkarte (`picture`, Helligkeit, Transparenz = schwarz, ≤ 1024 px),
      `Heightmap::normalized`
- [x] Plugin: „Eigenes Bild …“ (Dateidialog nach `/data/picked`), als Graustufen-PNG in
      `/data/textures` gemerkt, in der Galerie, „Entfernen“; „Glätten“ in % der Kachel
- [x] Eigene Textur im Export-Job geprüft (Kiesel-Foto auf Würfel)
- [ ] Dateidialog selbst nicht im Test geklickt (öffnet über das Portal auf dem echten Desktop)
- [x] Downloads mit Davis' Zustimmung (09.11.): 16 Poly-Haven-Höhenbilder (CC0, 28 MB) und
      `hero-patterns-2.1.0.tgz` (npm, nur als Text gelesen, nicht ausgeführt) – nur im Scratchpad
- [x] `scripts/build-textures.py` (Kontrast 0,5–99,5 %, 512 px, Hero-SVGs weiß auf schwarz per
      Inkscape) → 12 Natur + 32 Hero Patterns = 2,2 MB PNG in `assets/textures`, `CREDITS.md`,
      `catalog.rs` (per `include_bytes!` eingebettet; wasm 3,8 MB)
- [x] Galerie mit Gruppen (Muster / Natur & Material / Hero Patterns / Eigene), Quellenangabe unter
      der Textur und in der Notiz der Datei (CC BY verlangt Namensnennung), README-Credits
- [x] Weggelassen: Jeans, Fischgrät, Strick (zu wenig Relief bzw. nur Falten)
- [x] Favoriten (Stern)
## M6 Plugin-API 0.2 Teil 2 (Malen, Overlay, Performance) – Host-Session
- [x] Auftrag an die Host-Session (09.11.); zwei Commits (BVH, Farben je Dreieck), Rest in Arbeit:
      `MeshView.paint`/`overlay`, `MeshPick`/`MeshStroke`, Malmodus im Viewer, PreparedMesh
- [x] Teil 2 fertig (Commits bis f59b80f); Tests von mir nachgeprüft (114 + 228 + … grün)
- [x] Im echten Fenster gefunden: Malmodus fror ein (egui-Sperre doppelt genommen) → von der
      Host-Session behoben (b5f72e8), danach hier geklickt (`target/host/run.sh` zeigt das ganze Log,
      `RUST_LOG=warn,plugin=info,meshgrove=info` zeigt Plugin-Zeilen)
- [x] In der Testsitzung geklickt: Fläche markieren (nur hier / keine Textur), Umschalt+Klick nimmt
      zurück, Pinselstrich (Start/Move/End, Pinselkreis), Overlay orange, Auf Fläche legen
      (`out/panel-malen.png`)
- [ ] Ein Klick in den ersten Sekunden (Debug-Build, Suchbaum noch nicht fertig) ging einmal
      verloren: im Release-Build prüfen

## M7 Masken und Ebenen
- [x] Kern: `mask::Op` (Fläche füllen per Punkt + Normale, Pinsel-Tupfer, Leeren) – gilt für jede
      Netzauflösung; Radieren einer Fläche nimmt auch automatische Marken (innen/außen) zurück;
      `Mask::only_picked` (Ebene startet leer); `Textured.left_out`
- [x] Plugin: Werkzeuge unter der Vorschau (Drehen / Fläche markieren / Pinsel; keine Textur / nur
      hier Textur; Radius, Härte; Rückgängig, Wiederholen, Alle löschen), Overlay „orange = keine
      Textur“ aus dem Vorschau-Job, Striche je Modell und Ebene, bei Projekten in Teil-Koordinaten
- [x] Stempel: „Auftragen: Stempel“, Klick setzt ihn (schnappt auf die echte Oberfläche)
- [x] Ebenen (bis 4): Liste, ＋ Ebene (startet leer, Werkzeug „Fläche markieren – nur hier“),
      entfernen, verdecken/addieren; Qualität und Format gelten für alle
- [x] Über Jobs geprüft: Fläche + Pinsel ausgespart, Stempel, zwei Ebenen (`out/zwei-ebenen.png`);
      Ebenen-Bedienung in der Testsitzung geklickt (`out/panel-ebenen.png`)
- [x] Malen/Klicken im 3D-Fenster selbst: geklickt (siehe M6)
- [x] „Auf Fläche legen“: Klick dreht ein Ein-Netz-Modell so, dass die Fläche auf dem Bett liegt;
      „Wie in der Datei hinstellen“ nimmt es zurück (Projekte richtet man im Slicer aus)
- [x] Pinsel ist von sich aus präzise: Tupfer sind Geometrie und wirken auf dem verfeinerten Netz
- [x] Favoriten (★), Hinweis bei nicht geschlossenen Modellen

## M8 Erweiterte Projektionen
- [x] Box mit weichen Kanten, Zylinder/Umriss mit weichem Deckelübergang, Umriss, Stempel,
      Flip-Schutz, Tiefenwarnung; im Panel geklickt (Umriss am Kasten)
- [x] Hinweis im Panel, wenn ein Modell nicht sauber geschlossen ist

## M9 KI-Werkzeuge
- [x] Kern: `raster` (Bild ohne Grafikkarte: z-Puffer, Licht von links oben, Markiertes orange)
- [x] Plugin-Werkzeuge: `textures` (Liste + nummerierter Kontaktbogen), `inspect` (Größe,
      geschlossen, Teile, Innen-Anteil, Hinweise zu Kachel/Tiefe), `preview` (Bild vorher/nachher,
      Anteil ohne Textur, Warnungen), `apply` (Bündel mit Hintergrundjob `apply`, dessen Ergebnis das
      endgültige Bündel trägt); Recht `ai_tools` im Manifest
- [x] Mit `meshgrove-plugin-run` geprüft: `tools`, `inspect`, `preview` (0,3 s); CLI `--picture`
      zeigt dasselbe Bild (`out/ki-vorschau.png`)
- [ ] Host H (Jobs aus Werkzeugen): Auftrag an die Host-Session (09.11.), danach `apply` Ende zu
      Ende prüfen (plugin-run `call apply`, MCP, Chat)

## M10 Fertigstellen
- [ ] Grafiken (Krea 2 Turbo über ComfyUI), README, CHANGELOG, CI/Release
- [ ] **Test unter Linux ganz normal lokal** (Davis, 09.11.)
- [ ] **Test in der Windows-VM** (Davis, 09.11.: „vor allem auch“)
- [ ] Info an CNC Kitchen (Text entwerfen, Davis schickt)
