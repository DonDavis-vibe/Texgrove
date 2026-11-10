# Prüfplan Windows – Texgrove mit dem Plugin-API-0.2-Branch

Geprüft am 2026-11-10 in der VM `meshgrove-win11` (Windows 11 Enterprise Eval, Software-Renderer)
gegen Meshgrove-Branch `claude/happy-mahavira-d8760c` (ac194ea, Release-Build in der VM) und
Texgrove cea1849 (Teil 1) bzw. 754d5f5 und Host e4f6513, zuletzt 1a160de (Teil 2). Ziel: Fehler finden, die es unter Linux nicht gibt.

Wie geprüft wird: **A** automatisch per SSH ohne Oberfläche (`target/host/vm/tex-headless.ps1`,
MCP von Linux aus über SSH gesteuert) · **H** Screenshot-Hook mit gescripteten Klicks
(`target/host/vm/vmcase.sh`) – braucht das Konto `test` an der VM-Konsole angemeldet.

## Testumgebung
- `C:\dev\TexLib\Werkstatt – Prüfung\` (Gedankenstrich, Umlaut): `Würfel – groß.stl`, `rohr.stl`,
  `Adapter.STL` (Endung groß), `Helmhalter (Projekt).3mf` (Bambu-Projekt, 7 Objekte)
- Texgrove in `%APPDATA%\meshgrove\plugins\texgrove\`, Rechte: Dateien in die Bibliothek legen,
  Werkzeuge für die KI; „Bibliothek lesen“ aus (für die MCP-Läufe kurz „was die KI sehen darf“)
- Aufbau: `tex-setup.ps1` (Bibliothek + Plugin eintragen), `tex-grant.ps1 <none|ai-scope|all>`

## 1 Ohne Oberfläche
| Nr | Prüfung | Wie | Ergebnis |
|---|---|---|---|
| 1.1 | Branch baut unter Windows (GUI, STEP, MCP, plugin-run) | A | ✅ 19 + 6 min, keine Fehler |
| 1.2 | `inspect` und `preview` mit Namen wie `Würfel – groß.stl` | A | ✅ 0,1 s / 0,3 s, Bild kommt |
| 1.3 | `apply` als 3MF neben das Original | A | ✅ `Würfel – groß – Bricks.3mf`, Ziel `C:\dev\TexLib\Werkstatt – Prüfung\…`; **byte-gleich mit Linux** |
| 1.4 | `apply` als STL, nur außen | A | ✅ `rohr – Hexagons.stl`, byte-gleich mit Linux |
| 1.5 | Großes Modell: Adapter, Rändel, Normal (1 M Dreiecke) | A | ✅ 21 s (Linux 18 s) |
| 1.6 | Mehrteilige 3MF: Teileliste, ein Objekt tauschen (`derive_3mf`) | A | ✅ 7 Teile gelistet; getauschte Datei hat denselben Inhalt wie unter Linux – mit Host e4f6513 jetzt **byte-gleich** (vorher war nur das Betriebssystem-Byte der ZIP-Einträge anders) |
| 1.7 | MCP: ohne „Bibliothek lesen“ | A | ✅ klare Absage (unter Linux geprüft, gleicher Code) |
| 1.8 | MCP: `texgrove_inspect`, `texgrove_preview`, `texgrove_apply` | A | ✅ Bild kommt, Job startet, `meshgrove-mcp.exe` wartet nach dem Schließen 6 s auf den Job, Vorschlag „bin_2x1_6u – Waben“ steht in der Prüfung |

## 2 Im Fenster (10.11., Fenster 1280×800, Software-Renderer „Microsoft Basic Render Driver“)
| Nr | Prüfung | Wie | Ergebnis |
|---|---|---|---|
| 2.1 | Rechtsklick auf `Würfel – groß.stl` → „Textur auftragen …“ → Panel mit Vorschau, ohne „Bibliothek lesen“ | H | ✅ (`out/win-c04-panel.png`); Bingrove (API 0.1) läuft auf demselben Host weiter |
| 2.2 | Mehrfachauswahl + Rechtsklick: alle Modelle im Panel | H | ✅ drei Modelle dazu, das erste davon gewählt |
| 2.3 | „Textur auftragen“: Datei neben dem Original, Tag „Texturiert“; Rückgängig im Verlauf entfernt nur die Kopie | H | ✅ `Würfel – groß – Ziegel.3mf`, Tag in der Seitenleiste, Verlaufseintrag; Rückgängig nimmt genau diese Datei weg |
| 2.4 | Zweimal dieselbe Textur: zweiter Name frei | H | ✅ `… (2).3mf`, `(3)`, `(4)` · Groß-/Klein-Kollision: ✅ liegt `WÜRFEL – groß – ZIEGEL.3MF` schon da, heißt die neue Datei `Würfel – groß – Ziegel (2).3mf`, die alte bleibt unberührt |
| 2.5 | Vorschlag der KI (aus 1.8) in der Prüfung annehmen | H | ✅ „1 Datei neben bin_2x1_6u.stl in Gridfinity\Schublade“, nach „Alle annehmen“ liegt sie dort |
| 2.6 | 3MF-Projekt: Teileliste, Vorschau, Export | H | ✅ sieben Teile mit Haken, Vorschau an ihrem Platz, Export tauscht alle sieben Objekte (ZIP heil, in Texgrove wieder lesbar); im Slicer: Davis |
| 2.7 | Fläche markieren, Umschalt+Klick, Pinselstrich | H | ✅ mit grober Vorschau (66 k Dreiecke). ❗ Mit feiner Vorschau (526 k Dreiecke) schafft der Software-Renderer der VM kein Bild pro Sekunde mehr, Klicks kommen dann nicht an – liegt an der VM ohne Grafikkarte, nicht an Windows (unter Linux mit Software-Renderer genauso zäh) |
| 2.8 | „Datei wählen …“ und „Eigenes Bild …“ (Windows-Dateidialog) | H+M | ✅ der Windows-Dialog „Modell wählen“ geht auf, im Ordner der Bibliothek, Filter mit allen Endungen, `Adapter.STL` (Endung groß) wird gezeigt (`out/win-c34-dialog-vm.png`); eine Datei darin auswählen: Davis von Hand |
| 2.9 | Stapel: „Auf alle 4 Modelle auftragen“ | H | ✅ vier Dateien neben ihren Originalen, ein Verlaufseintrag, Adapter mit 1 M Dreiecken dabei |
| 2.10 | Meshgrove während eines Exports schließen, neu starten: Panel bedienbar | H | ✅ mitten im Stapel beendet: danach Knopf wieder da, kein hängender Fortschritt, keine halbe Datei in der Bibliothek |
| 2.11 | Pfad über 260 Zeichen als Quelle | A+H | ✅ Quelle mit 303 Zeichen über MCP gelesen und texturiert, in der Prüfung angenommen: `wuerfel_lang – Ziegel.3mf` liegt daneben (312 Zeichen) |

## 3 Release-Kandidat (10.11., Meshgrove 0.9.0 = cd98dbf, Texgrove 1.0.0 = da507ef)
| Nr | Prüfung | Wie | Ergebnis |
|---|---|---|---|
| 3.1 | Kandidat baut in der VM (GUI, STEP, MCP, plugin-run) | A | ✅ |
| 3.2 | Alle Fälle aus Teil 1 ohne Oberfläche | A | ✅ alle vier Ergebnisdateien (3MF, STL, Adapter mit 1 M Dreiecken, 3MF mit getauschtem Teil) byte-gleich mit Linux |
| 3.3 | Plugins-Menü mit zwei Plugins | H | ✅ „Bingrove“, „Texgrove“ – kein doppelter Name mehr |
| 3.4 | Knopf im Info-Bereich | H | ✅ „Textur auftragen …“ neben „In OrcaSlicer öffnen“ |

## Befunde
- Keine Windows-eigenen Fehler in Teil 1.
- Im Fenster gefunden und in Texgrove behoben (754d5f5): Ergebnis-Meldung stand unter dem Knopf
  außer Sicht → jetzt unter der Vorschau; „＋ Ebene“ war ein leeres Kästchen → „+“; Warnungen
  einer Vorschau blieben beim Wechsel des Modells stehen und fehlten nach einem Neustart;
  Warnungen pro 3MF-Teil doppelt → eine Zeile; „nicht geschlossen“ bei Modellen, die Meshgrove
  zum Lesen vereinfacht hat, war ein Fehlalarm.
- Im Fenster gefunden, an die Host-Session gemeldet und dort behoben (ad19322, c43598d, 1a160de):
  Zeilen brachen in einem geteilten Panel nicht um (Galerie abgeschnitten, Rollbalken über dem
  Knopf „Textur auftragen“); „✧“ war unter Windows ein leeres Kästchen; der Screenshot-Haken
  wartete in einem ruhenden Fenster ewig. Alle drei mit dem neuen Build in der VM nachgeprüft
  (`out/win-c30-neu.png`, `out/win-c35-lang.png`).
- `inspect` schrieb die Größe als `179.60000610351562` (auf beiden Systemen) → behoben (cea1849).
