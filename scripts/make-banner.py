#!/usr/bin/env python3
"""Makes assets/banner.png and assets/banner-2000.webp: the chosen draft with the name on it.

    python3 scripts/make-banner.py

The lettering is Red Hat Display Medium (SIL OFL, /usr/share/fonts/redhat on Fedora) in white on
a soft teal glow, as on Meshgrove's banner. Needs Pillow."""
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent.parent
DRAFT = ROOT / "assets/entwuerfe/banner_reihe_s8401.png"
FONT = "/usr/share/fonts/redhat/RedHatDisplay-Medium.otf"
NAME = "Texgrove"
# Where the name stands: left edge, the middle of its height, and how wide it may be (the parts
# begin at about x = 870).
LEFT, MIDDLE, WIDTH = 135, 440, 600
GLOW = (20, 132, 116)

banner = Image.open(DRAFT).convert("RGB")
size = 40
while ImageFont.truetype(FONT, size + 1).getbbox(NAME)[2] <= WIDTH:
    size += 1
font = ImageFont.truetype(FONT, size)
box = font.getbbox("Tx")  # the height of capitals and small letters, without the g's tail
top = MIDDLE - (box[1] + box[3]) // 2

# The glow: a blurred ellipse behind the left half of the name, laid on as light.
glow = Image.new("RGB", banner.size, (0, 0, 0))
centre = (LEFT + WIDTH * 0.42, MIDDLE + 10)
ImageDraw.Draw(glow).ellipse(
    (centre[0] - 330, centre[1] - 230, centre[0] + 330, centre[1] + 230), fill=GLOW
)
glow = glow.filter(ImageFilter.GaussianBlur(150))
banner = ImageChops.screen(banner, glow)

ImageDraw.Draw(banner).text((LEFT, top), NAME, font=font, fill=(255, 255, 255))
banner.save(ROOT / "assets/banner.png", optimize=True)
small = banner.resize((2000, round(2000 * banner.height / banner.width)), Image.LANCZOS)
small.save(ROOT / "assets/banner-2000.webp", quality=82, method=6)
print(f"{NAME} at {size} px, {banner.size[0]}x{banner.size[1]}")
