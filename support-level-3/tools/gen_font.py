#!/usr/bin/env python3
"""Génère l'atlas de police bitmap (PNG blanc + métriques JSON) pour l'UI du jeu."""
import json
import numpy as np
from PIL import Image, ImageDraw, ImageFont

OUT_DIR = "/home/z/my-project/support-level-3/assets/font"
FONT_PATH = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
SIZE = 34  # taille de rendu des glyphes

CHARS = (
    " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`"
    "abcdefghijklmnopqrstuvwxyz{|}~"
    "ÀÂÇÉÈÊËÎÏÔÖÙÛÜàâçéèêëîïôöùûüœŒ«»°²³€…–—'’"
)

font = ImageFont.truetype(FONT_PATH, SIZE)

# mesure
tmp = Image.new("L", (8, 8))
d = ImageDraw.Draw(tmp)

metrics = {}
tiles = []
W_TILE = 48
H_TILE = 48
COLS = 16
rows = (len(CHARS) + COLS - 1) // COLS
img = Image.new("RGBA", (COLS * W_TILE, rows * H_TILE), (0, 0, 0, 0))
draw = ImageDraw.Draw(img)

for i, ch in enumerate(CHARS):
    bbox = d.textbbox((0, 0), ch, font=font)
    w = bbox[2] - bbox[0]
    h = bbox[3] - bbox[1]
    asc, desc = font.getmetrics()
    col, row = i % COLS, i // COLS
    ox, oy = col * W_TILE, row * H_TILE
    draw.text((ox - bbox[0], oy - bbox[1]), ch, font=font, fill=(255, 255, 255, 255))
    metrics[ch] = {
        "x": ox, "y": oy, "w": W_TILE, "h": H_TILE,
        "gw": bbox[2], "gh": bbox[3], "gy": bbox[1],
        "adv": d.textlength(ch, font=font),
        "asc": asc,
    }

img.save(f"{OUT_DIR}/font_atlas.png")
with open(f"{OUT_DIR}/font_atlas.json", "w", encoding="utf-8") as f:
    json.dump({"tile_w": W_TILE, "tile_h": H_TILE, "cols": COLS, "size": SIZE, "glyphs": metrics}, f, ensure_ascii=False)

print(f"atlas: {img.size[0]}x{img.size[1]}, {len(CHARS)} glyphes")
