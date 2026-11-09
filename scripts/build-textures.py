#!/usr/bin/env python3
"""Builds the textures that ship with Texgrove from downloaded sources.

    scripts/build-textures.py <polyhaven-dir> <hero-svg-dir>

<polyhaven-dir> holds Poly Haven displacement maps as `<id>_disp_1k.png` or `<id>.png`
(https://polyhaven.com, CC0). <hero-svg-dir> holds Hero Patterns tiles as `<name>.svg`
(https://heropatterns.com by Steve Schoger, CC BY 4.0), as taken from the npm package
"hero-patterns". Inkscape renders the SVGs.

Writes assets/textures/<id>.png (8-bit grey, bright is high), assets/textures/CREDITS.md and
crates/texgrove-plugin/src/catalog.rs, the table the plugin embeds. The downloads themselves are
not part of the repository; the list below says what was taken.
"""

import os
import subprocess
import sys
import tempfile

import numpy as np
from PIL import Image, ImageFilter

ROOT = os.path.realpath(os.path.join(os.path.dirname(__file__), ".."))
OUT = os.path.join(ROOT, "assets", "textures")
CATALOG = os.path.join(ROOT, "crates", "texgrove-plugin", "src", "catalog.rs")

# Poly Haven id, German name, English name.
NATURE = [
    ("bark_brown_02", "Rinde", "Bark"),
    ("bark_willow_02", "Weidenrinde", "Willow bark"),
    ("rough_wood", "Raues Holz", "Rough wood"),
    ("leather_red_02", "Leder", "Leather"),
    ("cobblestone_floor_04", "Kopfsteinpflaster", "Cobblestones"),
    ("red_brick", "Mauer", "Brick wall"),
    ("roof_slates_03", "Schiefer", "Slate"),
    ("corrugated_iron", "Wellblech", "Corrugated iron"),
    ("coast_sand_rocks_02", "Fels", "Rock"),
    ("gravel_stones", "Kies", "Gravel"),
    ("rust_coarse_01", "Rost", "Rust"),
    ("metal_plate", "Blech", "Metal plate"),
]

# Hero Patterns name, German name, English name: the geometric ones that make good relief.
HERO = [
    ("jigsaw", "Puzzle", "Jigsaw"),
    ("overlappingCircles", "Kreise", "Circles"),
    ("overlappingHexagons", "Sechsecke", "Hexagon lines"),
    ("plus", "Plus", "Plus"),
    ("polkaDots", "Tupfen", "Polka dots"),
    ("wiggle", "Wellen", "Wiggle"),
    ("zigZag", "Zickzack", "Zig-zag"),
    ("circuitBoard", "Platine", "Circuit board"),
    ("topography", "Höhenlinien", "Topography"),
    ("temple", "Tempel", "Temple"),
    ("moroccan", "Marokko", "Moroccan"),
    ("houndstooth", "Hahnentritt", "Houndstooth"),
    ("diagonalLines", "Schräglinien", "Diagonal lines"),
    ("tinyCheckers", "Schachbrett", "Checkers"),
    ("floorTile", "Fliesen", "Floor tile"),
    ("parkayFloor", "Parkett", "Parquet"),
    ("aztec", "Azteken", "Aztec"),
    ("bathroomFloor", "Badfliesen", "Bathroom floor"),
    ("fourPointStars", "Sterne", "Stars"),
    ("graphPaper", "Karopapier", "Graph paper"),
    ("intersectingCircles", "Ringe", "Rings"),
    ("pixelDots", "Pixel", "Pixels"),
    ("signal", "Signal", "Signal"),
    ("steelBeams", "Stahlträger", "Steel beams"),
    ("ticTacToe", "Tic-Tac-Toe", "Tic-tac-toe"),
    ("wallpaper", "Tapete", "Wallpaper"),
    ("squaresInSquares", "Quadrate", "Squares"),
    ("roundedPlusConnected", "Gitter", "Lattice"),
    ("flippedDiamonds", "Rauten", "Diamonds"),
    ("endlessClouds", "Wolken", "Clouds"),
    ("leaf", "Blätter", "Leaves"),
    ("hideout", "Versteck", "Hideout"),
]

SIDE = 512


def stretch(values):
    """Spreads the values so the 0.5th and 99.5th percentile become 0 and 1."""
    low, high = np.percentile(values, [0.5, 99.5])
    if high - low < 1e-6:
        return np.zeros_like(values)
    return np.clip((values - low) / (high - low), 0.0, 1.0)


def nature(src_dir):
    made = []
    for ident, de, en in NATURE:
        path = next(
            (os.path.join(src_dir, n) for n in (f"{ident}_disp_1k.png", f"{ident}.png") if os.path.exists(os.path.join(src_dir, n))),
            None,
        )
        if path is None:
            print(f"missing: {ident}", file=sys.stderr)
            continue
        image = Image.open(path)
        # 16-bit greys come as mode I or I;16; colour and alpha are dropped for brightness.
        if image.mode in ("I", "I;16", "I;16B"):
            values = np.asarray(image, dtype=np.float64) / 65535.0
        else:
            values = np.asarray(image.convert("L"), dtype=np.float64) / 255.0
        grey = Image.fromarray((stretch(values) * 255.0).round().astype(np.uint8), "L")
        grey = grey.resize((SIDE, SIDE), Image.LANCZOS)
        grey.save(os.path.join(OUT, f"cc0_{ident}.png"), optimize=True)
        made.append((f"cc0_{ident}", de, en, "nature", 1.0, f"{ident} – Poly Haven (CC0)"))
    return made


def hero(svg_dir):
    made = []
    with tempfile.TemporaryDirectory() as tmp:
        for name, de, en in HERO:
            path = os.path.join(svg_dir, f"{name}.svg")
            if not os.path.exists(path):
                print(f"missing: {name}", file=sys.stderr)
                continue
            svg = open(path, encoding="utf-8").read()
            # Shapes white on black: what the pattern draws stands out.
            svg = svg.replace('fill="#000000"', 'fill="#ffffff"')
            white = os.path.join(tmp, "white.svg")
            open(white, "w", encoding="utf-8").write(svg)
            png = os.path.join(tmp, "out.png")
            # Rendered large, then shrunk: smooth edges without the blur of a small render.
            subprocess.run(
                ["inkscape", white, "--export-type=png", f"--export-filename={png}", "--export-width=1024",
                 "--export-background=#000000", "--export-background-opacity=1"],
                check=True, capture_output=True,
            )
            image = Image.open(png).convert("L")
            width, height = image.size
            aspect = height / width
            target = (SIDE, max(8, round(SIDE * aspect))) if aspect <= 1 else (max(8, round(SIDE / aspect)), SIDE)
            image = image.resize(target, Image.LANCZOS).filter(ImageFilter.GaussianBlur(0.6))
            values = stretch(np.asarray(image, dtype=np.float64) / 255.0)
            Image.fromarray((values * 255.0).round().astype(np.uint8), "L").save(
                os.path.join(OUT, f"hero_{name}.png"), optimize=True)
            made.append((f"hero_{name}", de, en, "hero", aspect, f"{name} – Hero Patterns by Steve Schoger (CC BY 4.0)"))
    return made


def write_catalog(entries):
    lines = [
        "//! The textures that ship with Texgrove, made by scripts/build-textures.py: do not edit.",
        "",
        "/// A texture from a picture.",
        "pub struct Shipped {",
        "    pub id: &'static str,",
        "    pub de: &'static str,",
        "    pub en: &'static str,",
        "    pub category: Category,",
        "    pub credit: &'static str,",
        "    pub png: &'static [u8],",
        "}",
        "",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]",
        "pub enum Category {",
        "    Nature,",
        "    Hero,",
        "}",
        "",
        "pub const SHIPPED: &[Shipped] = &[",
    ]
    for ident, de, en, category, _aspect, credit in entries:
        cat = "Nature" if category == "nature" else "Hero"
        lines.append(
            f'    Shipped {{ id: "{ident}", de: "{de}", en: "{en}", category: Category::{cat}, '
            f'credit: "{credit}", '
            f'png: include_bytes!("../../../assets/textures/{ident}.png") }},'
        )
    lines.append("];")
    open(CATALOG, "w", encoding="utf-8").write("\n".join(lines) + "\n")


def write_credits(entries):
    lines = [
        "# Texture credits",
        "",
        "The pictures in this folder were made from these sources by `scripts/build-textures.py`",
        "(scaled to 512 pixels, turned into greys, contrast spread).",
        "",
        "## Poly Haven (CC0 1.0, public domain)",
        "",
        "Displacement maps from [Poly Haven](https://polyhaven.com). Thank you!",
        "",
    ]
    for ident, _, en, category, _, _ in entries:
        if category == "nature":
            source = ident.removeprefix("cc0_")
            lines.append(f"- {en}: [{source}](https://polyhaven.com/a/{source})")
    lines += [
        "",
        "## Hero Patterns (CC BY 4.0)",
        "",
        "Patterns from [Hero Patterns](https://heropatterns.com) by Steve Schoger, licensed under",
        "[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Taken from the npm package",
        "[hero-patterns](https://github.com/lowmess/hero-patterns) (MIT, Alec Lomas); recoloured",
        "white on black and rasterised.",
        "",
    ]
    for ident, _, en, category, _, _ in entries:
        if category == "hero":
            lines.append(f"- {en}: {ident.removeprefix('hero_')}")
    open(os.path.join(OUT, "CREDITS.md"), "w", encoding="utf-8").write("\n".join(lines) + "\n")


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    os.makedirs(OUT, exist_ok=True)
    entries = nature(sys.argv[1]) + hero(sys.argv[2])
    # Pictures of textures no longer on the lists go.
    keep = {f"{e[0]}.png" for e in entries}
    for name in os.listdir(OUT):
        if name.endswith(".png") and name not in keep:
            os.remove(os.path.join(OUT, name))
    write_catalog(entries)
    subprocess.run(["rustfmt", "--edition", "2024", "--config", "max_width=120", CATALOG], check=False)
    write_credits(entries)
    size = sum(os.path.getsize(os.path.join(OUT, f"{e[0]}.png")) for e in entries)
    print(f"{len(entries)} textures, {size / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
