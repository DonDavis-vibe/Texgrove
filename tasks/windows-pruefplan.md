# Prüfplan Windows – Texgrove mit dem Plugin-API-0.2-Branch

Geprüft am 2026-11-10 in der VM `meshgrove-win11` (Windows 11 Enterprise Eval, Software-Renderer)
gegen Meshgrove-Branch `claude/happy-mahavira-d8760c` (ac194ea, Release-Build in der VM) und
Texgrove cea1849. Ziel: Fehler finden, die es unter Linux nicht gibt.

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
| 1.6 | Mehrteilige 3MF: Teileliste, ein Objekt tauschen (`derive_3mf`) | A | ✅ 7 Teile gelistet; getauschte Datei hat denselben Inhalt wie unter Linux (nur das Betriebssystem-Byte der ZIP-Einträge ist anders – an die Host-Session gemeldet) |
| 1.7 | MCP: ohne „Bibliothek lesen“ | A | ✅ klare Absage (unter Linux geprüft, gleicher Code) |
| 1.8 | MCP: `texgrove_inspect`, `texgrove_preview`, `texgrove_apply` | A | ✅ Bild kommt, Job startet, `meshgrove-mcp.exe` wartet nach dem Schließen 6 s auf den Job, Vorschlag „bin_2x1_6u – Waben“ steht in der Prüfung |

## 2 Im Fenster (offen – Konsole anmelden)
| Nr | Prüfung | Wie | Ergebnis |
|---|---|---|---|
| 2.1 | Rechtsklick auf `Würfel – groß.stl` → „Textur auftragen …“ → Panel mit Vorschau, ohne „Bibliothek lesen“ | H | – |
| 2.2 | Mehrfachauswahl + Rechtsklick: alle Modelle im Panel | H | – |
| 2.3 | „Textur auftragen“: Datei neben dem Original, Tag „Texturiert“, Notiz; Rückgängig im Verlauf entfernt nur die Kopie | H | – |
| 2.4 | Zweimal dieselbe Textur: zweiter Name frei (`… (2).3mf`); Groß-/Klein-Kollision (`würfel – groß – ziegel.3mf` liegt schon da) | H | – |
| 2.5 | Vorschlag aus 1.8 in der Prüfung annehmen | H | – |
| 2.6 | 3MF-Projekt: Teile anhaken, Vorschau, Export; Ergebnis im Slicer | H + Davis | – |
| 2.7 | Fläche markieren, Umschalt+Klick, Pinselstrich, Rückgängig; kein Einfrieren | H | – |
| 2.8 | „Datei wählen …“ und „Eigenes Bild …“ (Windows-Dateidialog) | M | – |
| 2.9 | Panel-Suche mit „Bibliothek lesen“ | H | – |
| 2.10 | Meshgrove während eines Exports schließen, neu starten: Panel bedienbar | H | – |
| 2.11 | Pfad über 260 Zeichen als Quelle | H | – |

## Befunde
- Keine Windows-eigenen Fehler in Teil 1.
- `inspect` schrieb die Größe als `179.60000610351562` (auf beiden Systemen) → behoben (cea1849).
