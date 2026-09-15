#!/usr/bin/env python3
"""Montage comparatif RT 4 modes (même point de vue, même partie type) :
OFF / Qualité (7 r/px) / Ultra (14 r/px) / Overdrive (26 r/px).
Bandeau = FPS mesurés sous lavapipe (A/B : même machine, seuls les rayons varient)."""
from PIL import Image, ImageDraw, ImageFont

CAP = "/home/z/my-project/download/captures"
OUT = "/home/z/my-project/download"

def load(name, size=(640, 360)):
    img = Image.open(f"{CAP}/{name}.png").convert("RGB")
    return img.resize(size, Image.LANCZOS)

try:
    font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 21)
    font_s = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 17)
except Exception:
    font = font_s = ImageFont.load_default()

FPS = {"off": "2.0 FPS", "qualite": "1.6 FPS", "ultra": "0.9 FPS", "overdrive": "0.6 FPS"}
shots = [
    ("rt_pt_off", "RT DÉSACTIVÉ", "0 rayon/px", "off"),
    ("rt_pt_qualite", "RT QUALITÉ", "7 rayons/px · ombres + AO", "qualite"),
    ("rt_pt_ultra", "RT ULTRA", "14 rayons/px · + GI 2 rebonds", "ultra"),
    ("rt_pt_overdrive", "RT OVERDRIVE · PATH TRACING", "26 rayons/px · Monte Carlo + NEE", "overdrive"),
]
W, H = 640, 360
LBL = 60
canvas = Image.new("RGB", (W * 4, H + LBL), (10, 10, 12))
d = ImageDraw.Draw(canvas)
for i, (f, label, sub, key) in enumerate(shots):
    canvas.paste(load(f), (i * W, LBL))
    tw = d.textlength(label, font=font)
    d.text((i * W + (W - tw) / 2, 4), label, fill=(235, 235, 235), font=font)
    tw2 = d.textlength(f"{sub}  ·  {FPS[key]} (lavapipe)", font=font_s)
    d.text((i * W + (W - tw2) / 2, 32), f"{sub}  ·  {FPS[key]} (lavapipe)", fill=(150, 200, 150), font=font_s)
canvas.save(f"{OUT}/comparaison_rt_off_qualite_ultra.png", optimize=True)
print("montage ->", f"{OUT}/comparaison_rt_off_qualite_ultra.png", canvas.size)
