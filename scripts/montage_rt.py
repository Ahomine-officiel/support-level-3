#!/usr/bin/env python3
"""Montage comparatif RT : OFF / Qualité / Ultra (même point de vue)."""
from PIL import Image, ImageDraw, ImageFont

CAP = "/home/z/my-project/download/captures"
OUT = "/home/z/my-project/download"

def load(name, size=(640, 360)):
    img = Image.open(f"{CAP}/{name}.png").convert("RGB")
    return img.resize(size, Image.LANCZOS)

try:
    font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 22)
except Exception:
    font = ImageFont.load_default()

shots = [
    ("rt_wow_off", "RT DÉSACTIVÉ"),
    ("rt_wow_qualite", "RT QUALITÉ"),
    ("rt_wow_ultra", "RT ULTRA"),
]
W, H = 640, 360
LBL = 34
canvas = Image.new("RGB", (W * 3, H + LBL), (10, 10, 12))
d = ImageDraw.Draw(canvas)
for i, (f, label) in enumerate(shots):
    canvas.paste(load(f), (i * W, LBL))
    tw = d.textlength(label, font=font)
    d.text((i * W + (W - tw) / 2, 6), label, fill=(230, 230, 230), font=font)
canvas.save(f"{OUT}/comparaison_rt_off_qualite_ultra.png", optimize=True)
print("montage ->", f"{OUT}/comparaison_rt_off_qualite_ultra.png", canvas.size)
