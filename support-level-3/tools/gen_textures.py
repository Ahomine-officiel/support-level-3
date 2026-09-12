#!/usr/bin/env python3
"""Génère toutes les textures PNG du jeu (256px, style sombre + usé)."""
import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont
import os

OUT = "/home/z/my-project/support-level-3/assets/textures"
os.makedirs(OUT, exist_ok=True)
S = 256
rng = np.random.default_rng(1337)

def font(sz, bold=False):
    name = "DejaVuSans-Bold.ttf" if bold else "DejaVuSans.ttf"
    return ImageFont.truetype(f"/usr/share/fonts/truetype/dejavu/{name}", sz)

def base(rgb, noise=14):
    a = np.zeros((S, S, 3), np.float32)
    a[:, :] = rgb
    a += rng.normal(0, noise, (S, S, 3))
    return np.clip(a, 0, 255).astype(np.uint8)

def stains(img, n=6, dark=38, blur=24):
    a = np.array(img, np.float32)
    for _ in range(n):
        x, y = rng.integers(0, S, 2)
        r = rng.integers(14, 55)
        yy, xx = np.mgrid[0:S, 0:S]
        d = np.sqrt((xx - x) ** 2 + (yy - y) ** 2)
        m = (d < r).astype(np.float32) * (1 - d / max(r, 1))
        a -= (m * dark)[..., None]
    a = np.clip(a, 0, 255)
    return Image.fromarray(a.astype(np.uint8)).filter(ImageFilter.GaussianBlur(2))

def save(img, name):
    img.convert("RGB").save(f"{OUT}/{name}.png")
    print("texture:", name)

# ---------- Murs ----------
img = stains(Image.fromarray(base((88, 92, 90))), n=9, dark=30)
d = ImageDraw.Draw(img)
for _ in range(7):  # fissures
    x, y = rng.integers(0, S, 2)
    for _ in range(rng.integers(4, 10)):
        nx, ny = x + rng.integers(-18, 18), y + rng.integers(4, 26)
        d.line([x, y, nx, ny], fill=(52, 54, 52), width=1)
        x, y = nx, ny
for yy in range(0, S, 64):  # joints de béton
    d.line([0, yy, S, yy], fill=(66, 69, 67), width=2)
save(img, "concrete")

img = stains(Image.fromarray(base((70, 74, 72), 10)), n=14, dark=45, blur=30)
save(img.filter(ImageFilter.GaussianBlur(1)), "concrete_dark")

# ---------- Sols ----------
img = stains(Image.fromarray(base((76, 78, 76), 12)), n=8, dark=26)
d = ImageDraw.Draw(img)
for _ in range(26):  # rayures
    x, y = rng.integers(0, S, 2)
    d.line([x, y, x + rng.integers(-30, 30), y + rng.integers(-30, 30)], fill=(96, 98, 94), width=1)
save(img, "floor_corridor")

img = Image.fromarray(base((66, 70, 76), 8))  # lino bureau
d = ImageDraw.Draw(img)
for i in range(0, S, 64):  # dalles 2x2
    d.rectangle([i, 0, i + 63, 63], outline=(50, 54, 60), width=2)
    d.rectangle([i, 64, i + 63, 127], outline=(50, 54, 60), width=2)
    d.rectangle([i, 128, i + 63, 191], outline=(50, 54, 60), width=2)
    d.rectangle([i, 192, i + 63, 255], outline=(50, 54, 60), width=2)
img = stains(img, n=7, dark=22)
save(img, "floor_office")

img = Image.fromarray(base((58, 62, 66), 10))  # sol technique serveurs
d = ImageDraw.Draw(img)
for i in range(0, S, 128):
    d.rectangle([i, 0, i + 127, 127], outline=(40, 44, 48), width=4)
    d.rectangle([i, 128, i + 127, 255], outline=(40, 44, 48), width=4)
    for k in range(4):
        d.rectangle([i + 8 + k * 30, 8, i + 20 + k * 30, 118], outline=(48, 52, 56), width=2)
img = stains(img, n=6, dark=18)
save(img, "floor_server")

img = stains(Image.fromarray(base((84, 72, 60), 12)), n=10, dark=30)  # archives : carrelage old-school
d = ImageDraw.Draw(img)
for i in range(0, S, 32):
    d.line([i, 0, i, S], fill=(64, 54, 44), width=1)
    d.line([0, i, S, i], fill=(64, 54, 44), width=1)
save(img, "floor_arch")

img = stains(Image.fromarray(base((96, 90, 80), 8)), n=5, dark=16)  # hall : lino propre-ish
d = ImageDraw.Draw(img)
for i in range(0, S, 128):
    d.rectangle([i, 0, i + 127, 127], outline=(74, 68, 60), width=3)
    d.rectangle([i, 128, i + 127, 255], outline=(74, 68, 60), width=3)
save(img, "floor_hall")

img = stains(Image.fromarray(base((78, 74, 70), 10)), n=9, dark=26)
d = ImageDraw.Draw(img)
for _ in range(4):  # taches d'huile
    x, y = rng.integers(0, S, 2)
    d.ellipse([x - 18, y - 10, x + 18, y + 10], fill=(52, 48, 44))
save(img, "floor_elec")

# ---------- Plafond ----------
img = Image.fromarray(base((58, 58, 56), 7))
d = ImageDraw.Draw(img)
for i in range(0, S, 128):
    d.rectangle([i, 0, i + 127, 127], outline=(42, 42, 40), width=3)
    d.rectangle([i, 128, i + 127, 255], outline=(42, 42, 40), width=3)
img = stains(img, n=8, dark=24)
save(img, "ceiling")

# ---------- Métaux ----------
img = Image.fromarray(base((34, 38, 42), 6))
d = ImageDraw.Draw(img)
for y in range(0, S, 8):  # metal brossé
    d.line([0, y, S, y], fill=(38, 42, 46), width=1)
img = stains(img, n=5, dark=12)
save(img, "metal_dark")

img = Image.fromarray(base((22, 24, 26), 5))  # devant de rack
d = ImageDraw.Draw(img)
for y in range(24, 232, 10):  # ouïes de ventilation
    d.line([30, y, 226, y], fill=(12, 13, 14), width=3)
save(img, "rack_front")

img = Image.fromarray(base((30, 32, 34), 4))  # face de baie serveur
d = ImageDraw.Draw(img)
d.rectangle([14, 14, 241, 60], fill=(16, 17, 18))  # slot serveur
d.rectangle([20, 70, 120, 78], fill=(48, 50, 52))  # poignée
for x in range(150, 230, 16):  # baïonnettes
    d.rectangle([x, 70, x + 10, 78], fill=(44, 46, 48))
for y in range(96, 240, 22):  # emplacements disques
    d.rectangle([14, y, 241, y + 14], fill=(20, 21, 22))
    d.rectangle([14, y, 30, y + 14], fill=(40, 42, 44))
save(img, "server_front")

img = Image.new("RGB", (S, S), (255, 255, 255))  # bande LED (teintée par instance)
d = ImageDraw.Draw(img)
d.line([0, 100, S, 100], fill=(230, 230, 230), width=8)
d.line([0, 140, S, 140], fill=(200, 200, 200), width=4)
save(img, "led_strip")

img = Image.fromarray(base((44, 48, 52), 5))  # écran éteint
d = ImageDraw.Draw(img)
d.polygon([(30, 40), (210, 60), (190, 220), (20, 200)], fill=(56, 62, 66))  # reflet
img = img.filter(ImageFilter.GaussianBlur(1))
save(img, "screen_off")

img = Image.new("RGB", (S, S), (8, 14, 8))  # écran terminal allumé
d = ImageDraw.Draw(img)
f = font(13)
lines = ["> mount /srv/level-3", "OK 6 nodes pending", "> ping admin", "no reply", "> ping admin", "no reply", "> ping admin", "he is replying from inside", "> _"]
for i, l in enumerate(lines):
    d.text((10, 8 + i * 26), l, fill=(90, 220, 110), font=f)
save(img, "screen_on")

img = Image.new("RGB", (S, S), (10, 12, 10))  # terminal RH
d = ImageDraw.Draw(img)
f = font(15, True)
d.text((12, 10), "NOTE DE FRAIS", fill=(120, 230, 130), font=f)
d.text((12, 30), "EXPENSE REPORT", fill=(120, 230, 130), font=font(12))
frs = font(11)
for i, l in enumerate(["Justificatifs : 4/4", "Montant : 42,73 EUR", "Statut : EN ATTENTE", "", "VALIDER ? [ENTREE]"]):
    d.text((12, 62 + i * 24), l, fill=(150, 235, 150), font=frs)
save(img, "terminal_screen")

# ---------- Mobilier ----------
img = stains(Image.fromarray(base((70, 52, 38), 8)), n=5, dark=16)  # bois bureau
d = ImageDraw.Draw(img)
for y in range(0, S, 6):  # grain
    d.line([0, y + rng.integers(-2, 3), S, y + rng.integers(-2, 3)], fill=(62, 46, 33), width=1)
save(img, "desk_wood")

img = Image.fromarray(base((96, 92, 88), 5))  # plastique kaiju beige
save(stains(img, 4, 10), "terminal_body")

img = Image.fromarray(base((40, 44, 58), 8))  # tissu chaise
save(stains(img, 4, 10), "chair_fabric")

img = Image.fromarray(base((58, 62, 68), 6))  # porte métal
d = ImageDraw.Draw(img)
d.rectangle([0, 190, S, 256], fill=(46, 50, 54))  # plinthe anti-choc
for x in (12, 243):  # charnières
    for y in (30, 120, 200):
        d.ellipse([x - 6, y - 4, x + 6, y + 4], fill=(38, 40, 44))
d.rectangle([196, 110, 228, 146], outline=(40, 42, 46), width=3)  # poignée
save(img, "door_metal")

img = Image.fromarray(base((36, 40, 44), 6))  # dormant de porte
save(stains(img, 4, 10), "door_frame")

img = Image.fromarray(base((54, 62, 72), 6))  # rayonnage
d = ImageDraw.Draw(img)
for _ in range(16):  # éclats de peinture
    x, y = rng.integers(0, S, 2)
    d.rectangle([x, y, x + 3, y + 3], fill=(120, 90, 60))
img = stains(img, n=6, dark=14)
save(img, "shelf_metal")

img = Image.fromarray(base((122, 88, 52), 8))  # carton
d = ImageDraw.Draw(img)
d.rectangle([0, 100, S, 156], fill=(138, 100, 60))  # ruban adhésif
d.rectangle([88, 30, 168, 70], outline=(90, 64, 38), width=2)  # étiquette
save(img, "cardboard")

# ---------- Entité / joueurs ----------
img = Image.fromarray(base((20, 19, 21), 4))  # costume de l'Auditeur
d = ImageDraw.Draw(img)
for y in range(0, S, 3):
    d.line([0, y, S, y], fill=(24, 23, 25), width=1)
save(img, "entity_cloth")

img = Image.new("RGB", (S, S), (196, 188, 176))  # masque pâle
d = ImageDraw.Draw(img)
a = np.array(img, np.float32)
a += rng.normal(0, 7, (S, S, 3))
img = Image.fromarray(np.clip(a, 0, 255).astype(np.uint8))
d = ImageDraw.Draw(img)
d.ellipse([60, 100, 100, 150], fill=(120, 112, 104))  # orbites
d.ellipse([156, 100, 196, 150], fill=(120, 112, 104))
d.line([128, 150, 128, 200], fill=(150, 140, 130), width=3)
save(img, "entity_mask")

img = Image.new("RGB", (S, S), (255, 255, 255))  # yeux émissifs
save(img, "eyes")

img = Image.fromarray(base((214, 140, 40), 10))  # gilet haute visibilité
d = ImageDraw.Draw(img)
d.rectangle([0, 60, S, 84], fill=(210, 210, 215))  # bandes réfléchissantes
d.rectangle([0, 150, S, 174], fill=(210, 210, 215))
d.line([118, 0, 138, S], fill=(160, 100, 30), width=8)
save(img, "tech_vest")

img = Image.fromarray(base((40, 46, 62), 8))  # pantalon
save(img, "tech_pants")

img = Image.fromarray(base((196, 158, 128), 8))  # tête
d = ImageDraw.Draw(img)
d.rectangle([0, 0, S, 70], fill=(52, 40, 34))  # cheveux
d.rectangle([60, 40, 196, 56], fill=(30, 30, 32))  # casque audio
save(img, "tech_head")

# ---------- Objets ----------
img = Image.new("RGB", (S, S), (240, 238, 230))  # justificatif
d = ImageDraw.Draw(img)
f = font(14)
for i in range(9):
    w = rng.integers(120, 220)
    d.line([20, 30 + i * 22, 20 + w, 30 + i * 22], fill=(150, 148, 142), width=2)
d.line([20, 228, 236, 228], fill=(90, 88, 84), width=3)
d.text((20, 4), "RECU / RECEIPT", fill=(70, 70, 70), font=font(13, True))
save(img, "receipt_paper")

img = Image.fromarray(base((110, 112, 108), 6))  # coffret disjoncteur
d = ImageDraw.Draw(img)
for y in range(0, S, 24):  # bandes danger haut/bas
    pass
d.rectangle([0, 0, S, 26], fill=(200, 170, 30))
for x in range(-20, S + 20, 40):
    d.polygon([(x, 0), (x + 20, 0), (x + 40, 26), (x + 20, 26)], fill=(30, 28, 24))
for i in range(4):  # interrupteurs
    d.rectangle([30 + i * 52, 90, 58 + i * 52, 170], fill=(40, 40, 42))
    d.rectangle([36 + i * 52, 100 + (0 if i % 2 else 40), 52 + i * 52, 160], fill=(180, 60, 40) if i % 2 else (60, 160, 70))
save(img, "breaker_panel")

img = Image.new("RGB", (S, S), (18, 20, 18))  # panneau EXIT émissif
d = ImageDraw.Draw(img)
d.text((28, 90), "SORTIE", fill=(90, 255, 120), font=font(40, True))
d.text((52, 150), "EXIT", fill=(90, 255, 120), font=font(34, True))
save(img, "exit_sign")

img = Image.fromarray(base((24, 26, 24), 5))  # batterie
d = ImageDraw.Draw(img)
d.rectangle([0, 110, S, 150], fill=(60, 200, 90))
d.text((16, 100), "+", fill=(220, 255, 220), font=font(28, True))
save(img, "battery")

img = Image.fromarray(base((160, 34, 30), 8))  # extincteur
d = ImageDraw.Draw(img)
d.rectangle([100, 20, 156, 60], fill=(30, 30, 32))
d.rectangle([92, 90, 164, 180], fill=(230, 226, 220))  # étiquette
save(img, "extinguisher")

img = Image.fromarray(base((120, 122, 118), 6))  # gaine ventilation
for y in range(0, S, 64):
    d = ImageDraw.Draw(img)
    d.line([0, y, S, y], fill=(90, 92, 88), width=4)
save(img, "duct")

# ---------- Affiches (humour support IT, bilingue) ----------
FG = (40, 40, 46)

def poster(name, lines, bg=(232, 228, 218), big=26):
    img = Image.new("RGB", (S, S), bg)
    d = ImageDraw.Draw(img)
    y = 24
    for (txt, f, col) in lines:
        d.text((16, y), txt, fill=col, font=f)
        y += f.size + 8
    d.rectangle([0, 0, S - 1, S - 1], outline=(120, 118, 112), width=4)
    save(img, name)

poster("poster_a", [
    ("ÇA NE MARCHE PAS ?", font(22, True), (180, 40, 40)),
    ("IT DOESN'T WORK?", font(18), (120, 118, 112)),
    ("", font(8), FG),
    ("Éteignez. Rallumez.", font(20, True), FG),
    ("Turn it off and on again.", font(16), FG),
    ("", font(8), FG),
    ("— Le Support, niveau -3", font(14), (90, 90, 96)),
])
poster("poster_b", [
    ("LE WIFI", font(26, True), FG),
    ("EST EN BAS.", font(26, True), FG),
    ("WIFI IS", font(18), (120, 118, 112)),
    ("DOWNSTAIRS.", font(18), (120, 118, 112)),
    ("▼", font(48, True), (180, 40, 40)),
])
poster("poster_c", [
    ("EN CAS D'URGENCE", font(19, True), (180, 40, 40)),
    ("IN CASE OF EMERGENCY", font(14), (120, 118, 112)),
    ("", font(8), FG),
    ("NE COURREZ PAS.", font(22, True), FG),
    ("DO NOT RUN.", font(18), FG),
    ("", font(8), FG),
    ("Il adore ça.", font(14), (90, 90, 96)),
])
poster("poster_d", [
    ("CONTRAT DE", font(20, True), FG),
    ("BAIL NUMÉRIQUE", font(20, True), FG),
    ("", font(8), FG),
    ("Clause 7.3 :", font(15), (90, 90, 96)),
    ("tout ticket non résolu", font(14), (90, 90, 96)),
    ("au bout de 40 ans", font(14), (90, 90, 96)),
    ("devient une personne.", font(14), (90, 90, 96)),
], bg=(210, 200, 180))

# ---------- Panneau danger ----------
img = Image.new("RGB", (S, S), (200, 170, 30))
d = ImageDraw.Draw(img)
for x in range(-20, S + 20, 40):
    d.polygon([(x, 0), (x + 20, 0), (x + 40, S), (x + 20, S)], fill=(30, 28, 24))
save(img, "hazard")

print("\nTextures générées dans", OUT)
