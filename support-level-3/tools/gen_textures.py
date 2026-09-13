#!/usr/bin/env python3
"""Génère toutes les textures PNG du jeu (style sombre + usé, SANS COUTURES).

Améliorations v2 :
- bruit de valeur périodique multi-octaves (fbm) -> textures tuilables sans raccord ;
- 512 px pour les grandes surfaces (murs, sols, plafond), 256 px pour le mobilier ;
- taches / fissures / rayures périodiques (distance torique) ;
- faux AO aux joints de dalles et au pied des murs ;
- même liste de noms de fichiers que v1 (aucun changement côté moteur).
"""
import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont
import os

OUT = "/home/z/my-project/support-level-3/assets/textures"
os.makedirs(OUT, exist_ok=True)
rng = np.random.default_rng(20260913)

_font_cache = {}
def font(sz, bold=False):
    key = (sz, bold)
    if key not in _font_cache:
        name = "DejaVuSans-Bold.ttf" if bold else "DejaVuSans.ttf"
        _font_cache[key] = ImageFont.truetype(f"/usr/share/fonts/truetype/dejavu/{name}", sz)
    return _font_cache[key]

# ---------------- bruit périodique ----------------
def value_noise(S, cells, seed):
    g = np.random.default_rng(seed).random((cells, cells)).astype(np.float32)
    coords = np.arange(S, dtype=np.float32) * cells / S
    i0 = np.floor(coords).astype(np.int64) % cells
    i1 = (i0 + 1) % cells
    t = coords - np.floor(coords)
    ts = (t * t * (3 - 2 * t)).astype(np.float32)
    fy = ts[:, None]
    fx = ts[None, :]
    a = g[np.ix_(i0, i0)]
    b = g[np.ix_(i0, i1)]
    c = g[np.ix_(i1, i0)]
    d = g[np.ix_(i1, i1)]
    return (a * (1 - fy) * (1 - fx) + b * (1 - fy) * fx
            + c * fy * (1 - fx) + d * fy * fx)

def fbm(S, seed, cells0=4, octaves=5, gain=0.55):
    out = np.zeros((S, S), np.float32)
    amp, tot = 1.0, 0.0
    for o in range(octaves):
        out += amp * value_noise(S, cells0 << o, seed + o * 977)
        tot += amp
        amp *= gain
    return out / tot

# ---------------- helpers périodiques (numpy, tuilables) ----------------
def to_arr(rgb, S, noise=12, seed=0):
    a = np.zeros((S, S, 3), np.float32)
    a[:, :] = rgb
    if noise:
        n = (fbm(S, seed, 8, 5) - 0.5) * 2 * noise
        g = np.random.default_rng(seed + 1).normal(0, noise * 0.35, (S, S, 1)).astype(np.float32)
        a += n[..., None] + g
    return a

def pstains(a, n, dark, blur=3.0, seed=7, rmin=20, rmax=90):
    """Taches sombres périodiques (distance torique -> pas de raccord)."""
    S = a.shape[0]
    yy, xx = np.mgrid[0:S, 0:S].astype(np.float32)
    mask = np.zeros((S, S), np.float32)
    r = np.random.default_rng(seed)
    for _ in range(n):
        x, y = r.integers(0, S, 2)
        rad = r.integers(rmin, rmax)
        dx = np.abs(xx - x); dx = np.minimum(dx, S - dx)
        dy = np.abs(yy - y); dy = np.minimum(dy, S - dy)
        d = np.sqrt(dx * dx + dy * dy)
        m = np.clip(1.0 - d / rad, 0, 1)
        m = m * m
        mask += m * r.uniform(0.5, 1.0)
    mask = np.clip(mask, 0, 3) / 3
    # flou périodique par convolution simple (roll)
    k = int(blur)
    acc = np.zeros_like(mask)
    for ox in range(-k, k + 1):
        for oy in range(-k, k + 1):
            acc += np.roll(np.roll(mask, ox, 0), oy, 1)
    mask = acc / acc.max()
    a -= (mask * dark)[..., None]
    return a

def pline(canvas, x0, y0, x1, y1, color, w=1, jitter=0.0, seed=3):
    """Ligne périodique (revient de l'autre côté du bord)."""
    S = canvas.shape[0]
    r = np.random.default_rng(seed)
    n = int(max(abs(x1 - x0), abs(y1 - y0)) * 2) + 2
    for t in np.linspace(0, 1, n):
        jx = r.normal(0, jitter) if jitter else 0.0
        jy = r.normal(0, jitter) if jitter else 0.0
        x = int(round(x0 + (x1 - x0) * t + jx))
        y = int(round(y0 + (y1 - y0) * t + jy))
        for dy in range(-w, w + 1):
            for dx in range(-w, w + 1):
                canvas[(y + dy) % S, (x + dx) % S] = color

def pgrid(a, step, color, w=1):
    """Grille de joints périodique (la ligne à 0 chevauche le bord)."""
    S = a.shape[0]
    for p in range(0, S, step):
        for k in range(w):
            a[(p + k) % S, :, :] = color
            a[:, (p + k) % S, :] = color

def ao_edges(a, width=6, strength=34):
    """Assombrit les bords de la tuile (faux AO aux joints)."""
    S = a.shape[0]
    ramp = np.linspace(1.0, 0.0, width, dtype=np.float32)
    m = np.ones((S, S), np.float32)
    for i, v in enumerate(ramp):
        m[i, :] = np.minimum(m[i, :], 1 - v)
        m[S - 1 - i, :] = np.minimum(m[S - 1 - i, :], 1 - v)
        m[:, i] = np.minimum(m[:, i], 1 - v)
        m[:, S - 1 - i] = np.minimum(m[:, S - 1 - i], 1 - v)
    a -= ((1 - m) * strength)[..., None]
    return a

def save(a, name, blur=0.0):
    S = a.shape[0]
    img = Image.fromarray(np.clip(a, 0, 255).astype(np.uint8))
    if blur:
        img = img.filter(ImageFilter.GaussianBlur(blur))
    img.convert("RGB").save(f"{OUT}/{name}.png")
    print("texture:", name, f"{S}px")

def S_of(name):
    big = {"concrete", "concrete_dark", "floor_corridor", "floor_office", "floor_server",
           "floor_arch", "floor_hall", "floor_elec", "ceiling"}
    return 512 if name in big else 256

# ================= GRANDES SURFACES (512, tuilables) =================

# ---------- mur béton ----------
S = 512
a = to_arr((92, 95, 92), S, 10, seed=100)
a = pstains(a, 26, 26, seed=101)
a = pstains(a, 8, 18, seed=102, rmax=200)            # grandes auréoles
# coulures d'eau verticales
water = fbm(S, 103, 6, 4)
a -= ((water > 0.62) * 22 * (water - 0.62) * 6)[..., None] * [0.7, 0.9, 1.0]
# fissures périodiques
for i in range(9):
    x, y = rng.integers(0, S, 2)
    for _ in range(rng.integers(5, 12)):
        nx, ny = x + rng.integers(-40, 40), y + rng.integers(8, 60)
        pline(a, x, y, nx, ny, (56, 58, 56), 1, seed=int(rng.integers(1 << 30)))
        x, y = nx % S, ny % S
# joints de coffrage horizontaux + trous de banche
pgrid(a, S // 2, (70, 73, 70), 3)
for (px, py) in ((S // 8, S // 4), (S * 5 // 8, S * 3 // 4)):
    a[py - 5:py + 5, px - 5:px + 5] = (48, 48, 46)
    a[py - 3:py + 3, px - 3:px + 3] = (36, 36, 35)
ao_edges(a, 8, 20)
save(a, "concrete", blur=0.6)

# ---------- béton sombre (piliers) ----------
a = to_arr((72, 76, 74), S, 8, seed=110)
a = pstains(a, 34, 40, seed=111)
a -= ((fbm(S, 112, 10, 4) > 0.6) * 14)[..., None]
save(a, "concrete_dark", blur=1.0)

# ---------- sol couloir : lino usé ----------
a = to_arr((80, 82, 80), S, 9, seed=120)
a = pstains(a, 22, 24, seed=121)
for _ in range(90):                                   # rayures claires/sombres
    x, y = rng.integers(0, S, 2)
    col = (100, 102, 98) if rng.random() < 0.6 else (58, 60, 58)
    pline(a, x, y, x + rng.integers(-70, 70), y + rng.integers(-70, 70), col, 1)
pgrid(a, S // 2, (60, 62, 60), 2)                     # lés de lino
ao_edges(a, 6, 18)
save(a, "floor_corridor", blur=0.4)

# ---------- sol bureau : dalles 2x2 ----------
a = to_arr((70, 76, 84), S, 7, seed=130)
tile = S // 2
grout = (44, 48, 56)
pgrid(a, tile, grout, 3)
# variation par dalle + marbrures
marb = fbm(S, 131, 8, 5)
for ty in range(2):
    for tx in range(2):
        sl = a[ty * tile:(ty + 1) * tile, tx * tile:(tx + 1) * tile]
        sl += rng.uniform(-7, 7)
        sl += ((marb[ty * tile:(ty + 1) * tile, tx * tile:(tx + 1) * tile] - 0.5) * 16)[..., None]
a = pstains(a, 14, 18, seed=132)
ao_edges(a, 10, 26)
save(a, "floor_office", blur=0.5)

# ---------- sol salle serveurs : faux plancher technique ----------
a = to_arr((60, 64, 70), S, 6, seed=140)
tile = S // 2
pgrid(a, tile, (36, 40, 46), 5)
for ty in range(2):
    for tx in range(2):
        x0, y0 = tx * tile, ty * tile
        # perforations rectangulaires sur une dalle sur deux
        if (tx + ty) % 2 == 0:
            for py in range(y0 + 24, y0 + tile - 16, 26):
                for px in range(x0 + 24, x0 + tile - 16, 26):
                    a[py:py + 12, px:px + 12] = (26, 29, 33)
                    a[py:py + 3, px:px + 12] = (48, 54, 60)
        # vis aux coins
        for (vx, vy) in ((x0 + 12, y0 + 12), (x0 + tile - 16, y0 + 12),
                         (x0 + 12, y0 + tile - 16), (x0 + tile - 16, y0 + tile - 16)):
            a[vy:vy + 5, vx:vx + 5] = (88, 92, 96)
            a[vy + 1:vy + 4, vx + 1:vx + 4] = (40, 42, 46)
a = pstains(a, 10, 16, seed=141)
save(a, "floor_server", blur=0.3)

# ---------- sol archives : carrelage vieilli ----------
a = to_arr((96, 84, 70), S, 8, seed=150)
tile = S // 8
pgrid(a, tile, (62, 52, 42), 2)
shade = fbm(S, 151, 5, 3)
a += ((shade - 0.5) * 22)[..., None]
a = pstains(a, 26, 30, seed=152)
for _ in range(30):                                   # carreaux éclatés
    x, y = rng.integers(0, S, 2)
    pline(a, x, y, x + rng.integers(-20, 20), y + rng.integers(-20, 20), (70, 60, 50), 1)
ao_edges(a, 6, 14)
save(a, "floor_arch", blur=0.4)

# ---------- sol hall : lino clair ----------
a = to_arr((108, 100, 90), S, 6, seed=160)
tile = S // 2
pgrid(a, tile, (78, 72, 64), 3)
a = pstains(a, 10, 14, seed=161)
speck = np.random.default_rng(162).random((S, S))
a += ((speck > 0.995) * 26)[..., None]
ao_edges(a, 8, 18)
save(a, "floor_hall", blur=0.4)

# ---------- sol local électrique : béton + huile + marquage ----------
a = to_arr((82, 80, 76), S, 9, seed=170)
a = pstains(a, 18, 22, seed=171)
for _ in range(6):                                    # flaques d'huile
    x, y = rng.integers(0, S, 2)
    r = rng.integers(24, 60)
    yy, xx = np.mgrid[0:S, 0:S].astype(np.float32)
    dx = np.abs(xx - x); dx = np.minimum(dx, S - dx)
    dy = np.abs(yy - y); dy = np.minimum(dy, S - dy)
    d = np.sqrt(dx * dx + dy * dy)
    m = np.clip(1 - d / r, 0, 1) ** 1.5
    a -= (m * 40)[..., None] * [1.0, 1.05, 1.2]
# bande jaune de sécurité au centre
a[:, S // 2 - 6:S // 2 - 2] = (168, 142, 34)
a[:, S // 2 + 2:S // 2 + 6] = (168, 142, 34)
for _ in range(40):
    pline(a, rng.integers(0, S), rng.integers(0, S), rng.integers(0, S), rng.integers(0, S), (64, 62, 58), 1)
save(a, "floor_elec", blur=0.5)

# ---------- plafond : dalles acoustiques ----------
a = to_arr((64, 64, 61), S, 5, seed=180)
tile = S // 2
pgrid(a, tile, (44, 44, 42), 4)
holes = np.random.default_rng(181).random((S, S))
a -= ((holes > 0.93) * 10)[..., None]
a = pstains(a, 12, 20, seed=182, rmax=150)            # auréoles d'eau
# auréole brune marquée sur une dalle
yy, xx = np.mgrid[0:S, 0:S].astype(np.float32)
d = np.sqrt((xx - 3 * S / 4) ** 2 + (yy - S / 4) ** 2)
ring = np.clip(1 - np.abs(d - 70) / 26, 0, 1)
a -= (ring * 26)[..., None] * [1.15, 1.0, 0.75]
save(a, "ceiling", blur=0.5)

# ================= MÉTAUX (256) =================
S = 256
a = to_arr((36, 40, 44), S, 5, seed=200)
for y in range(0, S, 4):                              # métal brossé
    v = rng.integers(-3, 4)
    a[y, :, :] += v
a = pstains(a, 12, 14, seed=201)
rust = fbm(S, 202, 6, 5)
a += ((rust > 0.68) * 24)[..., None] * [1.6, 0.9, 0.4]  # points de rouille
save(a, "metal_dark", blur=0.5)

# ---------- devant de rack ----------
a = to_arr((26, 28, 30), S, 4, seed=210)
for y in range(30, 226, 12):                          # ouïes
    a[y:y + 4, 34:222] = (13, 14, 15)
    a[y + 4:y + 6, 34:222] = (44, 47, 50)
a = pstains(a, 6, 10, seed=211)
a[0:8, :] = (18, 20, 22); a[-8:, :] = (18, 20, 22)
save(a, "rack_front")

# ---------- face de baie serveur ----------
a = to_arr((32, 34, 37), S, 4, seed=220)
units = 4
for u in range(units):
    y0 = 8 + u * 60
    a[y0:y0 + 52, 10:246] = (18, 19, 21)              # châssis unité
    a[y0 + 2:y0 + 10, 16:240] = (28, 30, 33)          # bandeau
    a[y0 + 3:y0 + 9, 20:120] = (44, 47, 51)           # poignée longue
    for k in range(6):                                # baïonnettes
        a[y0 + 3:y0 + 9, 150 + k * 14:158 + k * 14] = (52, 55, 58)
    for dslot in range(3):                            # disques
        a[y0 + 18:y0 + 44, 16 + dslot * 76:88 + dslot * 76] = (22, 23, 25)
        a[y0 + 20:y0 + 26, 20 + dslot * 76:36 + dslot * 76] = (40, 43, 46)
        # LED (t;colorées mais discrètes, l'émissive vient de led_strip)
        a[y0 + 30:y0 + 36, 22 + dslot * 76:26 + dslot * 76] = (90, 160, 90)
a = pstains(a, 5, 8, seed=221)
save(a, "server_front")

# ---------- bande LED (blanche, teintée par instance) ----------
a = np.full((128, 128, 3), 235, np.float32)
a += np.random.default_rng(230).normal(0, 8, (128, 128, 3))
a[40:88, :] = 250
save(a, "led_strip")

# ---------- écran éteint ----------
a = to_arr((40, 44, 50), S, 3, seed=240)
yy, xx = np.mgrid[0:S, 0:S].astype(np.float32)
refl = np.clip(1 - np.abs((xx * 0.7 + yy * 0.7) - S * 0.7) / 90, 0, 1)
a += (refl * 26)[..., None]                            # reflet diagonal
a = pstains(a, 5, 6, seed=241, rmax=50)
save(a, "screen_off", blur=0.6)

# ---------- écran terminal allumé ----------
a = np.zeros((S, S, 3), np.float32); a[:, :] = (7, 13, 8)
img = Image.fromarray(np.clip(a, 0, 255).astype(np.uint8))
d = ImageDraw.Draw(img)
lines = [
    ("> ssh admin@lvl-3", (90, 220, 110), 13),
    ("connexio(n) OK", (70, 180, 90), 12),
    ("> ping serveur-01", (90, 220, 110), 13),
    ("64 bytes : time=40ms", (70, 180, 90), 12),
    ("> ping serveur-02", (90, 220, 110), 13),
    ("pas de réponse", (150, 90, 70), 12),
    ("> ping serveur-03", (90, 220, 110), 13),
    ("il répond depuis l'intérieur", (150, 90, 70), 12),
    ("> ls /mensch", (90, 220, 110), 13),
    ("vous  vous  vous  vous_", (60, 255, 90), 13),
]
y = 10
for (txt, col, sz) in lines:
    d.text((12, y), txt, fill=col, font=font(sz))
    y += sz + 8
arr = np.array(img, np.float32)
arr[::3, :, :] *= 0.82                                  # scanlines
arr[:, :, :] *= (1 - 0.35 * (np.mgrid[0:S, 0:S][0] / S) ** 2)[..., None]  # vignette
save(arr, "screen_on")

# ---------- écran terminal RH ----------
img = Image.new("RGB", (S, S), (10, 13, 11))
d = ImageDraw.Draw(img)
d.text((14, 12), "SGRH v4.2 — PAIE", fill=(110, 230, 130), font=font(17, True))
d.text((14, 34), "HR SYSTEM v4.2 — PAY", fill=(80, 170, 95), font=font(12))
d.rectangle([12, 56, 244, 58], fill=(60, 140, 75))
rows = [("Justificatifs", "4/4", (150, 235, 150)),
        ("Montant", "42,73 EUR", (150, 235, 150)),
        ("Statut", "EN ATTENTE", (230, 190, 90)),
        ("Délai", "40 ANS", (150, 235, 150))]
for i, (k, v, c) in enumerate(rows):
    d.text((16, 70 + i * 26), k, fill=(95, 160, 105), font=font(12))
    d.text((130, 70 + i * 26), v, fill=c, font=font(12))
d.rectangle([60, 186, 196, 218], outline=(120, 235, 130), width=2)
d.text((74, 194), "VALIDER [E]", fill=(140, 245, 150), font=font(13, True))
arr = np.array(img, np.float32)
arr[::3, :, :] *= 0.85
save(arr, "terminal_screen")

# ================= MOBILIER =================
# ---------- bois de bureau ----------
S = 256
warp = fbm(S, 300, 4, 4)
a = to_arr((74, 55, 40), S, 6, seed=301)
grain = (np.sin((np.mgrid[0:S, 0:S][1] + warp * 26) * 0.55) * 0.5 + 0.5)
a -= (grain * 12)[..., None]
a = pstains(a, 6, 12, seed=302)
for _ in range(6):                                    # rayures
    x, y = rng.integers(0, S, 2)
    pline(a, x, y, x + rng.integers(-60, 60), y + rng.integers(-8, 8), (58, 42, 30), 1)
save(a, "desk_wood", blur=0.4)

# ---------- plastique beige ----------
a = to_arr((150, 146, 136), S, 6, seed=310)
a = pstains(a, 6, 10, seed=311)
speck = np.random.default_rng(312).random((S, S))
a -= ((speck > 0.99) * 16)[..., None]
a[0:6, :] *= 0.85                                     # chanfrein simulé
save(a, "terminal_body", blur=0.4)

# ---------- tissu de chaise ----------
a = to_arr((46, 50, 64), S, 6, seed=320)
weave = (np.mgrid[0:S, 0:S][0] % 4 < 2) ^ (np.mgrid[0:S, 0:S][1] % 4 < 2)
a -= (weave * 8)[..., None]
a = pstains(a, 7, 12, seed=321)
save(a, "chair_fabric", blur=0.3)

# ---------- porte métal ----------
a = to_arr((60, 64, 70), S, 5, seed=330)
a[186:256, :] = (48, 52, 56)                          # plinthe anti-choc
a[186:189, :] = (30, 33, 36)
for x in (14, 226):                                   # charnières
    for y in (36, 120, 196):
        a[y - 6:y + 6, x - 6:x + 6] = (42, 44, 48)
        a[y - 4:y + 4, x - 4:x + 4] = (56, 59, 63)
a[108:148, 192:232] = (74, 78, 84)                    # rosace de poignée
a[120:136, 200:224] = (46, 49, 54)
for _ in range(24):                                   # rayures d'usage
    x, y = rng.integers(0, S, 2)
    pline(a, x, y, x + rng.integers(-30, 30), y + rng.integers(-10, 10), (74, 78, 84), 1)
a = pstains(a, 6, 10, seed=331)
save(a, "door_metal", blur=0.3)

# ---------- dormant ----------
a = to_arr((40, 44, 48), S, 5, seed=340)
a = pstains(a, 8, 12, seed=341)
save(a, "door_frame", blur=0.4)

# ---------- rayonnage ----------
a = to_arr((58, 66, 76), S, 5, seed=350)
for _ in range(30):                                   # éclats de peinture
    x, y = rng.integers(0, S, 2)
    a[y:y + 3, x:x + 3] = (110, 84, 56)
a = pstains(a, 10, 14, seed=351)
a[0:4, :] = (34, 40, 48); a[-4:, :] = (34, 40, 48)
save(a, "shelf_metal")

# ---------- carton ----------
a = to_arr((128, 94, 56), S, 7, seed=360)
corr = (np.mgrid[0:S, 0:S][1] % 6 < 1)
a -= (corr * 6)[..., None]
a[100:156, :] = (146, 108, 66)                        # ruban
a[100:104, :] = (170, 134, 88); a[152:156, :] = (112, 80, 48)
d = Image.fromarray(np.clip(a, 0, 255).astype(np.uint8))
dd = ImageDraw.Draw(d)
dd.rectangle([86, 28, 172, 72], fill=(226, 220, 206))
dd.text((92, 32), "SL3", fill=(70, 66, 60), font=font(16, True))
dd.text((92, 52), "ARCHIVES-3", fill=(90, 86, 80), font=font(10))
dd.text((96, 180), "FRAGILE", fill=(150, 60, 40), font=font(13, True))
a = np.array(d, np.float32)
a = pstains(a, 7, 12, seed=361)
save(a, "cardboard", blur=0.3)

# ================= ENTITÉ / JOUEURS =================
# ---------- costume de l'Auditeur ----------
a = to_arr((24, 23, 26), S, 4, seed=370)
stripes = (np.mgrid[0:S, 0:S][1] % 6 < 1)
a += (stripes * 5)[..., None] * [1.0, 1.0, 1.2]       # filigrane sombre
a = pstains(a, 8, 10, seed=371, rmax=40)
save(a, "entity_cloth", blur=0.3)

# ---------- masque pâle ----------
a = to_arr((206, 198, 184), S, 5, seed=380)
yy, xx = np.mgrid[0:S, 0:S].astype(np.float32)
cx, cy = S / 2, S / 2
face = np.clip(1 - ((xx - cx) / (S * 0.42)) ** 2 - ((yy - cy) / (S * 0.48)) ** 2, 0, 1)
a -= ((1 - face) * 60)[..., None]                      # forme ovale plus sombre hors visage
for ex in (cx - 34, cx + 34):                          # orbites creuses
    d = np.sqrt((xx - ex) ** 2 + (yy - cy - 10) ** 2)
    a -= (np.clip(1 - d / 26, 0, 1) * 120)[..., None]
    a -= (np.clip(1 - d / 44, 0, 1) * 40)[..., None]
dm = np.sqrt((xx - cx) ** 2 + (yy - cy - 78) ** 2)     # bouche cousue
a -= (np.clip(1 - dm / 30, 0, 1) * 70)[..., None]
for k in range(5):                                     # points de suture
    a[int(cy) + 64:int(cy) + 92, int(cx) - 24 + k * 12:int(cx) - 20 + k * 12] = (110, 96, 84)
crack = fbm(S, 381, 12, 4)
a -= ((crack > 0.72) * 26)[..., None]                  # craquelures
save(a, "entity_mask", blur=0.5)

# ---------- yeux émissifs ----------
a = np.full((64, 64, 3), 255, np.float32)
a[20:44, 12:30] = (120, 20, 16)                        # iris sombre (émissive x2.6)
a[20:44, 34:52] = (120, 20, 16)
save(a, "eyes")

# ---------- gilet haute visibilité ----------
a = to_arr((216, 140, 40), S, 7, seed=390)
a[58:84, :] = (216, 216, 220); a[58:61, :] = (180, 180, 186)
a[148:174, :] = (216, 216, 220); a[148:151, :] = (180, 180, 186)
a[:, 108:132] = (150, 96, 28)                          # fermeture
img = Image.fromarray(np.clip(a, 0, 255).astype(np.uint8))
d = ImageDraw.Draw(img)
d.text((16, 18), "SL-3", fill=(60, 50, 30), font=font(20, True))
d.text((16, 200), "TECH", fill=(90, 70, 34), font=font(12, True))
a = np.array(img, np.float32)
a = pstains(a, 5, 10, seed=391)
save(a, "tech_vest")

# ---------- pantalon ----------
a = to_arr((44, 50, 66), S, 6, seed=400)
a = pstains(a, 6, 10, seed=401)
save(a, "tech_pants", blur=0.3)

# ---------- tête technicien ----------
a = to_arr((198, 160, 130), S, 6, seed=410)
a[0:74, :] = (58, 44, 36)                              # cheveux
a[38:58, :] = (32, 32, 34)                             # bandeau casque
yy, xx = np.mgrid[0:S, 0:S].astype(np.float32)
for ex in (78, 178):                                   # yeux
    d = np.sqrt((xx - ex) ** 2 + (yy - 116) ** 2)
    a -= (np.clip(1 - d / 12, 0, 1) * 90)[..., None]
a[158:170, 88:168] = (150, 106, 92)                    # bouche
a = pstains(a, 3, 6, seed=411, rmax=30)
save(a, "tech_head", blur=0.4)

# ================= OBJETS =================
# ---------- justificatif ----------
img = Image.new("RGB", (S, S), (238, 234, 224))
d = ImageDraw.Draw(img)
d.text((18, 6), "REÇU / RECEIPT", fill=(60, 60, 66), font=font(15, True))
d.rectangle([18, 24, 238, 26], fill=(90, 90, 94))
for i in range(8):
    w = rng.integers(120, 216)
    d.line([18, 40 + i * 20, 18 + w, 40 + i * 20], fill=(148, 146, 140), width=2)
d.rectangle([18, 208, 238, 240], outline=(120, 118, 112), width=1)
d.text((24, 214), "42,73 EUR", fill=(60, 60, 66), font=font(15, True))
d.rectangle([0, 0, S - 1, S - 1], outline=(200, 196, 186), width=2)
save(np.array(img, np.float32), "receipt_paper")

# ---------- coffret disjoncteur ----------
a = to_arr((112, 114, 110), S, 5, seed=420)
a[0:30, :] = (196, 164, 30)
for x in range(-20, S + 20, 40):
    xs = np.arange(S)
    for i in range(30):
        c = (30, 28, 24) if ((x + i) // 20) % 2 == 0 else (196, 164, 30)
        a[i, max(0, x + i):max(0, x + i) + 1] = c
for i in range(4):                                    # interrupteurs
    x0 = 26 + i * 56
    a[84:170, x0:x0 + 30] = (44, 44, 46)
    on = i % 2 == 0
    a[100 if on else 128:160 if on else 158, x0 + 5:x0 + 25] = (200, 60, 40) if not on else (70, 170, 80)
img = Image.fromarray(np.clip(a, 0, 255).astype(np.uint8))
d = ImageDraw.Draw(img)
d.text((12, 40), "TGBT — NIVEAU -3", fill=(50, 52, 50), font=font(13, True))
d.text((12, 186), "DANGER HAUTE TENSION", fill=(120, 30, 20), font=font(12, True))
a = np.array(img, np.float32)
a = pstains(a, 6, 10, seed=421)
save(a, "breaker_panel", blur=0.3)

# ---------- panneau sortie ----------
img = Image.new("RGB", (S, S), (14, 30, 16))
d = ImageDraw.Draw(img)
d.rectangle([8, 8, S - 8, S - 8], outline=(70, 200, 100), width=3)
d.text((38, 26), "SORTIE", fill=(96, 255, 130), font=font(44, True))
d.text((66, 84), "EXIT", fill=(96, 255, 130), font=font(36, True))
# pictogramme bonhomme qui court
d.ellipse([112, 140, 144, 172], fill=(96, 255, 130))
d.line([128, 172, 108, 208], fill=(96, 255, 130), width=8)
d.line([108, 208, 128, 236], fill=(96, 255, 130), width=8)
d.line([128, 182, 160, 196], fill=(96, 255, 130), width=8)
d.line([128, 182, 96, 200], fill=(96, 255, 130), width=8)
save(np.array(img, np.float32), "exit_sign")

# ---------- batterie ----------
a = to_arr((30, 34, 32), 128, 5, seed=440)
a[52:76, :] = (62, 200, 92)
a[44:52, 40:88] = (90, 210, 110)
d = Image.fromarray(np.clip(a, 0, 255).astype(np.uint8))
ImageDraw.Draw(d).text((10, 8), "+", fill=(210, 255, 215), font=font(30, True))
save(np.array(d, np.float32), "battery")

# ---------- extincteur ----------
a = to_arr((164, 36, 30), S, 6, seed=450)
a[88:178, 88:170] = (232, 228, 220)                   # étiquette
img = Image.fromarray(np.clip(a, 0, 255).astype(np.uint8))
d = ImageDraw.Draw(img)
d.rectangle([96, 22, 160, 58], fill=(38, 38, 40))     # poignée
d.text((94, 96), "FEU", fill=(160, 40, 30), font=font(17, True))
d.text((94, 116), "FIRE", fill=(160, 40, 30), font=font(14, True))
d.text((94, 140), "NIVEAU -3", fill=(90, 88, 84), font=font(11))
a = np.array(img, np.float32)
a = pstains(a, 5, 8, seed=451, rmax=40)
save(a, "extinguisher", blur=0.3)

# ---------- gaine ventilation ----------
a = to_arr((118, 120, 116), S, 5, seed=460)
for y in range(0, S, 48):                             # nervures
    a[y:y + 5, :] = (86, 88, 84)
    a[y + 5:y + 8, :] = (140, 142, 138)
a = pstains(a, 8, 12, seed=461)
save(a, "duct", blur=0.4)

# ---------- diffuseur de néon (grille prismatique) ----------
a = np.full((128, 128, 3), 248, np.float32)
for y in range(0, 128, 16):
    a[y:y + 2, :] = (210, 212, 208)
for x in range(0, 128, 16):
    a[:, x:x + 2] = (210, 212, 208)
a = a * (0.94 + 0.06 * fbm(128, 470, 8, 3))[..., None]
save(a, "light_panel")

# ---------- bandes danger ----------
a = np.zeros((256, 256, 3), np.float32)
xs = np.arange(256)
for y in range(256):
    phase = (xs + y) // 32 % 2
    a[y, xs[phase == 0]] = (198, 166, 30)
    a[y, xs[phase == 1]] = (34, 30, 24)
save(a, "hazard")

# ================= AFFICHES =================
def poster(name, blocks, bg=(226, 220, 206), border=(120, 118, 112)):
    S = 256
    img = Image.new("RGB", (S, S), bg)
    d = ImageDraw.Draw(img)
    # texture papier légère
    arr = np.array(img, np.float32)
    arr += np.random.default_rng(sum(ord(c) for c in name)).normal(0, 4, (S, S, 3))
    img = Image.fromarray(np.clip(arr, 0, 255).astype(np.uint8))
    d = ImageDraw.Draw(img)
    y = 22
    for (txt, sz, bold, col) in blocks:
        d.text((20, y), txt, fill=col, font=font(sz, bold))
        y += sz + 9
    d.rectangle([0, 0, S - 1, S - 1], outline=border, width=4)
    # ruban adhésif aux coins
    d.polygon([(0, 0), (44, 0), (0, 44)], fill=(200, 196, 180))
    d.polygon([(S, S), (S - 44, S), (S, S - 44)], fill=(200, 196, 180))
    save(np.array(img, np.float32), name)

RED = (178, 44, 38)
GREY = (118, 116, 110)
INK = (44, 44, 50)
poster("poster_a", [
    ("ÇA NE MARCHE PAS ?", 21, True, RED),
    ("IT DOESN'T WORK?", 15, False, GREY),
    ("Éteignez. Rallumez.", 19, True, INK),
    ("Turn it off and on again.", 14, False, INK),
    ("— Le Support, niveau -3", 13, False, GREY),
])
poster("poster_b", [
    ("LE WIFI", 30, True, INK),
    ("EST EN BAS.", 30, True, INK),
    ("WIFI IS DOWNSTAIRS.", 14, False, GREY),
    ("▼", 52, True, RED),
])
poster("poster_c", [
    ("EN CAS D'URGENCE", 18, True, RED),
    ("IN CASE OF EMERGENCY", 13, False, GREY),
    ("NE COURREZ PAS.", 21, True, INK),
    ("DO NOT RUN.", 17, False, INK),
    ("Il adore ça.", 14, False, GREY),
    ("He loves it.", 12, False, GREY),
], bg=(216, 208, 192))
poster("poster_d", [
    ("CONTRAT DE BAIL", 19, True, INK),
    ("NUMÉRIQUE", 19, True, INK),
    ("DIGITAL LEASE", 12, False, GREY),
    ("Clause 7.3 :", 14, False, GREY),
    ("tout ticket non résolu", 13, False, GREY),
    ("au bout de 40 ans", 13, False, GREY),
    ("devient une personne.", 13, False, GREY),
], bg=(206, 198, 178))

print("\nTextures générées dans", OUT)

