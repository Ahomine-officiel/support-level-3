#!/usr/bin/env python3
"""Génère les modèles 3D du jeu au format GLTF 2.0 (.gltf + .bin).

v2 — modèles « regardables » :
- boîtes CHANFREINÉES (6 faces + 12 arêtes + 8 coins) : les arêtes accrochent la lumière ;
- orientation automatique des polygones (normale sortante garantie, Newell) ;
- boîtes rotatives en lacet (yaw) pour objets inclinés / éparpillés ;
- cylindres plus fins, détails : poignées, grilles, pieds, câbles, gouttières ;
- noms de modèles ET de matériaux inchangés (aucun changement côté serveur/client).
"""
import json, math, os, struct

OUT = "/home/z/my-project/support-level-3/assets/models"
os.makedirs(OUT, exist_ok=True)

# ---------------- outils géométriques ----------------
def vadd(a, b): return (a[0] + b[0], a[1] + b[1], a[2] + b[2])
def vsub(a, b): return (a[0] - b[0], a[1] - b[1], a[2] - b[2])
def vmul(a, s): return (a[0] * s, a[1] * s, a[2] * s)
def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])
def norm(a):
    l = math.sqrt(sum(x * x for x in a)) or 1.0
    return (a[0] / l, a[1] / l, a[2] / l)

EX, EY, EZ = (1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0)

def newell(pts):
    n = [0.0, 0.0, 0.0]
    for i in range(len(pts)):
        p, q = pts[i], pts[(i + 1) % len(pts)]
        n[0] += (p[1] - q[1]) * (p[2] + q[2])
        n[1] += (p[2] - q[2]) * (p[0] + q[0])
        n[2] += (p[0] - q[0]) * (p[1] + q[1])
    return n

class Builder:
    """Assemble boîtes/cylindres ; chaque ajout = une primitive (un matériau)."""

    def __init__(self):
        self.pos, self.nrm, self.uv, self.idx = [], [], [], []
        self.prim_ranges = []  # (mat, idx_start, idx_count)

    # ---- bas niveau ----
    def _vert(self, p, n, uv):
        self.pos.append(p); self.nrm.append(n); self.uv.append(uv)
        return len(self.pos) - 1

    def poly(self, mat, pts, uvs, hint):
        """Polygone plan avec orientation sortante automatique."""
        if dot(newell(pts), hint) < 0:
            pts = pts[::-1]
            uvs = uvs[::-1]
        n = norm(newell(pts))
        s = len(self.pos)
        i0 = len(self.idx)
        for p, uv in zip(pts, uvs):
            self._vert(p, n, uv)
        for k in range(1, len(pts) - 1):
            self.idx += [s, s + k, s + k + 1]
        self.prim_ranges.append((mat, i0, len(self.idx) - i0))

    def merge_sub(self, sub, mat):
        base_pos, base_idx = len(self.pos), len(self.idx)
        self.pos += sub.pos; self.nrm += sub.nrm; self.uv += sub.uv
        self.idx += [i + base_pos for i in sub.idx]
        self.prim_ranges.append((mat, base_idx, len(self.idx) - base_idx))

    # ---- boîte simple (murs/sols : tuiles exactes) ----
    def box(self, mat, size, t=(0, 0, 0), uv=None):
        sx, sy, sz = (s * 0.5 for s in size)
        x, y, z = t
        if uv is None:
            uv = lambda d: (d[0] / 2.0, d[1] / 2.0)
        elif isinstance(uv, (int, float)):
            uv = lambda d, uv=uv: (uv, uv)
        hw, hh, hd = size
        faces = [
            (EX, (0, 1, 2), (hd, hh), (x + sx, y, z)),   # +X : (u,v)=(Z,Y)
            (vmul(EX, -1), (2, 1, 0), (hd, hh), (x - sx, y, z)),
            (EY, (2, 0, 1), (hw, hd), (x, y + sy, z)),   # +Y : (X,Z)
            (vmul(EY, -1), (0, 1, 2), (hw, hd), (x, y - sy, z)),
            (EZ, (0, 1, 2), (hw, hh), (x, y, z + sz)),   # +Z : (X,Y)
            (vmul(EZ, -1), (1, 0, 2), (hw, hh), (x, y, z - sz)),
        ]
        for n_ax, perm, (a, b), c in faces:
            u = [0.0, 0.0, 0.0]; v = [0.0, 0.0, 0.0]
            u[perm[0]], v[perm[1]] = a, b
            # base orthonormale avec u x v = n
            if dot(cross(u, v), n_ax) < 0:
                u, v = v, u
            pts = [vadd(vadd(c, vmul(u, sa)), vmul(v, sv)) for (sa, sv) in ((-a, -b), (a, -b), (a, b), (-a, b))]
            uvs = [(0, 0), (uv((a * 2, b * 2))[0], 0), uv((a * 2, b * 2)), (0, uv((a * 2, b * 2))[1])]
            self.poly(mat, pts, uvs, n_ax)

    # ---- boîte chanfreinée ----
    def chbox(self, mat, size, t=(0, 0, 0), ch=0.025, uv=None):
        hx, hy, hz = (s * 0.5 for s in size)
        c = min(ch, hx * 0.45, hy * 0.45, hz * 0.45)
        x0, y0, z0 = t
        if uv is None:
            uvf = lambda d: (d[0] / 2.0, d[1] / 2.0)
        else:
            uvf = lambda d: (uv, uv)
        ax = (hx, hy, hz)
        ctr = (x0, y0, z0)

        def pt(xi, si, fi, xj, sj, fj, xk, sk, fk):
            """Point : pour chaque axe (xi,xj,xk), si*axe à h (fi) ou h-c (sinon)."""
            d = [0.0, 0.0, 0.0]
            d[xi] = si * (ax[xi] if fi else ax[xi] - c)
            d[xj] = sj * (ax[xj] if fj else ax[xj] - c)
            d[xk] = sk * (ax[xk] if fk else ax[xk] - c)
            return vadd(ctr, d)

        axes = [EX, EY, EZ]
        # 6 faces
        for i in range(3):
            for si in (1, -1):
                j, k = (i + 1) % 3, (i + 2) % 3
                n = vmul(axes[i], si)
                # base (u,v) avec u x v = n
                u, v = axes[j], axes[k]
                if dot(cross(u, v), n) < 0:
                    u, v = v, u
                a, b = ax[j] - c, ax[k] - c
                cen = vadd(ctr, vmul(axes[i], si * ax[i]))
                pts = [vadd(vadd(cen, vmul(u, sa)), vmul(v, sv)) for (sa, sv) in ((-a, -b), (a, -b), (a, b), (-a, b))]
                uu, vv = uvf((a * 2, b * 2))
                uvs = [(0, 0), (uu, 0), (uu, vv), (0, vv)]
                self.poly(mat, pts, uvs, n)
        # 12 arêtes (chaque paire non ordonnée une seule fois)
        seen = set()
        for i in range(3):
            for j in range(3):
                if i == j:
                    continue
                for si in (1, -1):
                    for sj in (1, -1):
                        if (j, sj, i, si) in seen:
                            continue
                        seen.add((i, si, j, sj))
                        k = ({0, 1, 2} - {i, j}).pop()
                        n = norm(vadd(vmul(axes[i], si), vmul(axes[j], sj)))
                        v1 = pt(i, si, True, j, sj, False, k, 1, False)
                        v2 = pt(i, si, True, j, sj, False, k, -1, False)
                        v3 = pt(j, sj, True, i, si, False, k, -1, False)
                        v4 = pt(j, sj, True, i, si, False, k, 1, False)
                        ln = (ax[k] - c) * 2
                        uu = uvf((ln, c * 2))[0]
                        pts = [v1, v2, v3, v4]
                        uvs = [(0, 0), (0, uu), (uu, uu), (uu, 0)]
                        self.poly(mat, pts, uvs, n)
        # 8 coins
        for si in (1, -1):
            for sj in (1, -1):
                for sk in (1, -1):
                    n = norm((si, sj, sk))
                    p1 = pt(0, si, True, 1, sj, False, 2, sk, False)
                    p2 = pt(1, sj, True, 0, si, False, 2, sk, False)
                    p3 = pt(2, sk, True, 0, si, False, 1, sj, False)
                    self.poly(mat, [p1, p2, p3], [(0, 0), (c, 0), (c, c)], n)

    # ---- boîte tournée en lacet ----
    def box_yaw(self, mat, size, t=(0, 0, 0), yaw=0.0, ch=None, uv=None):
        sub = Builder()
        if ch:
            sub.chbox(mat, size, (0, 0, 0), ch, uv)
        else:
            sub.box(mat, size, (0, 0, 0), uv)
        ca, sa = math.cos(yaw), math.sin(yaw)
        for i in range(len(sub.pos)):
            px, py, pz = sub.pos[i]
            nx, ny, nz = sub.nrm[i]
            sub.pos[i] = (px * ca - pz * sa + t[0], py + t[1], px * sa + pz * ca + t[2])
            sub.nrm[i] = (nx * ca - nz * sa, ny, nx * sa + nz * ca)
        self.merge_sub(sub, mat)

    # ---- cylindre ----
    def cyl(self, mat, r, h, seg=16, t=(0, 0, 0), caps=True):
        cx, cy, cz = t
        ring = [(math.sin(2 * math.pi * i / seg), math.cos(2 * math.pi * i / seg)) for i in range(seg)]
        s0 = len(self.idx)
        for i in range(seg):
            j = (i + 1) % seg
            ax, az = ring[i]; bx, bz = ring[j]
            n = norm(((ax + bx) * 0.5, 0.0, (az + bz) * 0.5))
            v0 = (cx + ax * r, cy, cz + az * r)
            v1 = (cx + bx * r, cy, cz + bz * r)
            v2 = (cx + bx * r, cy + h, cz + bz * r)
            v3 = (cx + ax * r, cy + h, cz + az * r)
            s = len(self.pos)
            self.pos += [v0, v1, v2, v3]
            self.nrm += [n] * 4
            self.uv += [(i / seg, 0), ((i + 1) / seg, 0), ((i + 1) / seg, 1), (i / seg, 1)]
            self.idx += [s, s + 1, s + 2, s, s + 2, s + 3]
        if caps:
            for (yy, flip) in ((cy, False), (cy + h, True)):
                cs = len(self.pos)
                self.pos.append((cx, yy, cz))
                self.nrm.append((0.0, 1.0 if yy > cy else -1.0, 0.0))
                self.uv.append((0.5, 0.5))
                for i in range(seg):
                    ax, az = ring[i]
                    self.pos.append((cx + ax * r, yy, cz + az * r))
                    self.nrm.append((0.0, 1.0 if yy > cy else -1.0, 0.0))
                    self.uv.append((0.5 + ax * 0.5, 0.5 + az * 0.5))
                for i in range(seg):
                    a, b = cs + 1 + i, cs + 1 + (i + 1) % seg
                    self.idx += ([cs, a, b] if not flip else [cs, b, a])
        self.prim_ranges.append((mat, s0, len(self.idx) - s0))

    # ---- export GLTF ----
    def export(self, name):
        mats = []
        for (mat, _, _) in self.prim_ranges:
            if mat not in mats:
                mats.append(mat)
        blob = bytearray()

        def add_view(data, target=None):
            while len(blob) % 4:
                blob.append(0)
            off = len(blob)
            blob.extend(data)
            v = {"buffer": 0, "byteOffset": off, "byteLength": len(data)}
            if target:
                v["target"] = target
            views.append(v)
            return len(views) - 1

        views, accs = [], []
        pack = lambda vals, comps: b"".join(struct.pack("<%df" % comps, *v) for v in vals)
        v_p = add_view(pack(self.pos, 3), 34962)
        v_n = add_view(pack(self.nrm, 3), 34962)
        v_t = add_view(pack(self.uv, 2), 34962)
        v_i = add_view(b"".join(struct.pack("<I", i) for i in self.idx), 34963)
        fmin = lambda vals: [min(v[i] for v in vals) for i in range(3)]
        fmax = lambda vals: [max(v[i] for v in vals) for i in range(3)]
        accs.append({"bufferView": v_p, "componentType": 5126, "count": len(self.pos), "type": "VEC3", "min": fmin(self.pos), "max": fmax(self.pos)})
        accs.append({"bufferView": v_n, "componentType": 5126, "count": len(self.nrm), "type": "VEC3"})
        accs.append({"bufferView": v_t, "componentType": 5126, "count": len(self.uv), "type": "VEC2"})
        accs.append({"bufferView": v_i, "componentType": 5125, "count": len(self.idx), "type": "SCALAR"})
        prim_list = []
        for (mat, start, count) in self.prim_ranges:
            accs.append({"bufferView": v_i, "byteOffset": start * 4, "componentType": 5125, "count": count, "type": "SCALAR"})
            prim_list.append({
                "attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2},
                "indices": len(accs) - 1,
                "material": mats.index(mat),
            })
        gltf = {
            "asset": {"version": "2.0", "generator": "sl3-tools-v2"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"mesh": 0, "name": name}],
            "meshes": [{"primitives": prim_list, "name": name}],
            "materials": [{"name": m} for m in mats],
            "accessors": accs,
            "bufferViews": views,
            "buffers": [{"uri": name + ".bin", "byteLength": len(blob)}],
        }
        with open(f"{OUT}/{name}.gltf", "w") as f:
            json.dump(gltf, f, indent=1)
        with open(f"{OUT}/{name}.bin", "wb") as f:
            f.write(blob)
        tris = len(self.idx) // 3
        print(f"modèle: {name} ({len(self.pos)} sommets, {tris} tris, matériaux: {', '.join(mats)})")

def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]

# ---------------- structures du monde (boîtes simples, tuilables) ----------------
w = Builder()
w.box("concrete", (2.0, 3.0, 0.24), (0, 1.5, 0))
w.export("wall")

p = Builder()
p.chbox("concrete_dark", (0.5, 3.0, 0.5), (0, 1.5, 0), 0.03)
p.box("concrete_dark", (0.62, 0.12, 0.62), (0, 0.06, 0))      # socle
p.box("concrete_dark", (0.58, 0.08, 0.58), (0, 2.96, 0))      # couronnement
p.export("pillar")

for nm, tex in [("floor_hall", "floor_hall"), ("floor_corridor", "floor_corridor"),
                ("floor_office", "floor_office"), ("floor_server", "floor_server"),
                ("floor_arch", "floor_arch"), ("floor_elec", "floor_elec")]:
    b = Builder()
    b.box(tex, (2.0, 0.1, 2.0), (0, -0.05, 0), uv=1.0)
    b.export(nm)

c = Builder()
c.box("ceiling", (2.0, 0.1, 2.0), (0, 3.05, 0), uv=1.0)
c.export("ceil")

# ---------------- mobilier / props (chanfreinés, détaillés) ----------------

# néon : boîtier + diffuseur prismatique + verrous
lf = Builder()
lf.chbox("metal_dark", (1.3, 0.08, 0.42), (0, -0.04, 0), 0.015)
lf.chbox("light_panel", (1.2, 0.035, 0.3), (0, 0.005, 0), 0.008)
for sx in (-0.61, 0.61):                                       # embouts
    lf.box("metal_dark", (0.06, 0.1, 0.44), (sx, -0.045, 0))
lf.export("light_fixture")

# rack décoratif : montants + panneaux latéraux ouïes + porte avant + plinthe
rk = Builder()
for sx in (-0.88, 0.88):                                       # montants
    rk.chbox("metal_dark", (0.08, 2.15, 0.66), (sx, 1.075, 0.02), 0.012)
rk.chbox("metal_dark", (1.9, 0.08, 0.75), (0, 2.11, 0), 0.012) # capot
rk.chbox("metal_dark", (1.9, 0.1, 0.75), (0, 0.05, 0), 0.012)  # base
rk.box("rack_front", (1.68, 1.9, 0.03), (0, 1.075, -0.36))     # porte
for y in (0.55, 1.075, 1.6):                                   # traverses
    rk.box("metal_dark", (1.68, 0.05, 0.03), (0, y, -0.37))
rk.box("metal_dark", (0.04, 2.15, 0.62), (-0.93, 1.075, 0.03)) # flancs
rk.box("metal_dark", (0.04, 2.15, 0.62), (0.93, 1.075, 0.03))
rk.export("rack")

# baie serveur interactive : 3 unités, LED par unité (teintée par instance),
# gâtière câbles au-dessus, socle
sb = Builder()
sb.chbox("metal_dark", (1.5, 2.0, 0.8), (0, 1.0, 0), 0.02)
sb.box("server_front", (1.3, 1.8, 0.04), (0, 1.0, -0.41))
for u in range(3):                                             # LED par unité
    sb.box("led_strip", (1.26, 0.045, 0.02), (0, 0.42 + u * 0.6, -0.437))
sb.chbox("metal_dark", (1.2, 0.14, 0.5), (0, 2.06, 0.05), 0.02)  # gouttière
for cz in (-0.08, 0.0, 0.08):                                  # câbles sortants
    sb.cyl("metal_dark", 0.02, 0.36, 6, (0.3, 1.86, cz))
sb.box("metal_dark", (1.56, 0.08, 0.86), (0, 0.04, 0))         # socle
sb.export("server_bay")

# bureau complet : plateau, panneau modestie, piètement, tour PC, écran incliné,
# clavier, souris, mug, chaise à 5 branches avec accoudoirs
ds = Builder()
ds.chbox("desk_wood", (1.6, 0.06, 0.8), (0, 0.75, -0.1), 0.012)
ds.box("metal_dark", (1.5, 0.5, 0.03), (0, 0.47, -0.47))       # panneau modestie
for lx in (-0.72, 0.72):                                       # piètement en T
    ds.box("metal_dark", (0.06, 0.72, 0.06), (lx, 0.39, -0.1))
    ds.box("metal_dark", (0.06, 0.06, 0.7), (lx, 0.03, -0.05))
# tour PC sous le bureau + LED
ds.box_yaw("metal_dark", (0.22, 0.45, 0.46), (0.55, 0.255, -0.12), 0.08)
ds.box("led_strip", (0.02, 0.03, 0.02), (0.455, 0.42, -0.33))
# écran : pied + col + cadre incliné (léger yaw) + dalle émissive
ds.box("metal_dark", (0.26, 0.02, 0.2), (0, 0.79, -0.3))
ds.box("metal_dark", (0.05, 0.32, 0.05), (0, 0.95, -0.3))
ds.box_yaw("metal_dark", (0.6, 0.38, 0.035), (0, 1.25, -0.28), -0.06)
ds.box_yaw("screen_on", (0.55, 0.33, 0.012), (0, 1.25, -0.262), -0.06)
# clavier + souris + mug
ds.chbox("metal_dark", (0.42, 0.025, 0.15), (0, 0.79, 0.05), 0.006)
ds.chbox("metal_dark", (0.07, 0.02, 0.11), (0.3, 0.788, 0.06), 0.008)
ds.cyl("chair_fabric", 0.04, 0.09, 10, (-0.35, 0.78, 0.02))    # mug
# chaise : assise + dossier incliné + accoudoirs + colonne + 5 branches + roulettes
ds.chbox("chair_fabric", (0.46, 0.07, 0.46), (0, 0.48, 0.62), 0.015)
ds.box_yaw("chair_fabric", (0.45, 0.54, 0.07), (0, 0.82, 0.85), 0.12)
for ax in (-0.26, 0.26):                                       # accoudoirs
    ds.box("metal_dark", (0.05, 0.03, 0.3), (ax, 0.68, 0.6))
    ds.box("metal_dark", (0.05, 0.18, 0.05), (ax, 0.58, 0.72))
ds.cyl("metal_dark", 0.05, 0.36, 10, (0, 0.06, 0.62))          # colonne
for k in range(5):                                             # étoile
    a = 2 * math.pi * k / 5
    bx, bz = 0.62 + 0.22 * math.sin(a), 0.62 + 0.22 * math.cos(a)  # (x,z) autour de (0, .62)
    dx, dz = bx - 0.0, bz - 0.62
    yaw = math.atan2(dz, dx)
    ln = math.hypot(dx, dz)
    ds.box_yaw("metal_dark", (ln, 0.035, 0.05), (dx / 2, 0.035, 0.62 + dz / 2), -yaw)
    ds.cyl("metal_dark", 0.03, 0.035, 8, (dx, 0.0, dz + 0.62)) # roulette
ds.export("desk_set")

# porte : dormants + linteau + joint d'étanchéité + seuil
df = Builder()
for sx in (-0.62, 0.62):
    df.chbox("door_frame", (0.12, 2.2, 0.16), (sx, 1.1, 0), 0.012)
    df.box("metal_dark", (0.03, 2.1, 0.02), (sx - (0.07 if sx < 0 else -0.07), 1.1, -0.08))
df.chbox("door_frame", (1.36, 0.12, 0.16), (0, 2.26, 0), 0.012)
df.box("metal_dark", (1.12, 0.03, 0.14), (0, 0.015, 0))        # seuil
df.export("door_frame")

# panneau de porte : vantail + plinthe basse + poignée + hublot
dp = Builder()
dp.chbox("door_metal", (1.12, 2.2, 0.08), (0, 1.1, 0), 0.012)
dp.box("metal_dark", (1.12, 0.28, 0.02), (0, 0.16, -0.05))     # plinthe anti-choc
dp.cyl("metal_dark", 0.025, 0.1, 8, (0.42, 1.05, -0.06))       # rosace
dp.box("metal_dark", (0.14, 0.035, 0.035), (0.5, 1.05, -0.05)) # levier
dp.box("screen_off", (0.24, 0.4, 0.02), (-0.2, 1.55, -0.045))  # hublot vitré
dp.box("metal_dark", (0.28, 0.44, 0.012), (-0.2, 1.55, -0.043))
dp.export("door_panel")

# porte de sortie (hall) + panneau émissif
ed = Builder()
for sx in (-3.5, 3.5):
    ed.chbox("door_frame", (0.14, 2.5, 0.2), (sx, 1.25, 0), 0.015)
ed.chbox("door_frame", (7.14, 0.16, 0.2), (0, 2.58, 0), 0.015)
ed.chbox("exit_sign", (1.3, 0.42, 0.06), (0, 2.95, 0.05), 0.008)
ed.box("metal_dark", (7.0, 0.06, 0.1), (0, 0.03, 0))           # seuil
ed.export("exit_door")

ep = Builder()
for sx in (-1.72, 1.72):                                       # 2 vantaux coulissants
    ep.chbox("door_metal", (3.3, 2.5, 0.1), (sx, 1.25, 0), 0.015)
    ep.box("metal_dark", (3.3, 0.06, 0.03), (sx, 1.15, -0.065))# barre anti-panique
ep.export("exit_panel")

# rayonnage d'archives : montants, 4 tablettes, contreventement diagonal,
# cartons / registres variés
sh = Builder()
for sx in (-0.95, 0.95):
    sh.chbox("shelf_metal", (0.05, 2.0, 0.5), (sx, 1.0, 0), 0.008)
for y in (0.3, 0.85, 1.4, 1.95):
    sh.chbox("shelf_metal", (1.9, 0.04, 0.5), (0, y, 0), 0.006)
sh.box_yaw("shelf_metal", (0.04, 2.1, 0.04), (-0.4, 1.0, 0.22), 1.15)   # diagonale
sh.box_yaw("shelf_metal", (0.04, 2.1, 0.04), (0.45, 1.0, -0.22), -1.2)
sh.box_yaw("cardboard", (0.5, 0.35, 0.4), (-0.5, 0.5, 0.02), 0.12, ch=0.012)
sh.box_yaw("cardboard", (0.45, 0.3, 0.35), (0.42, 1.05, 0.04), -0.2, ch=0.012)
sh.box_yaw("cardboard", (0.4, 0.28, 0.3), (0.08, 1.62, -0.02), 0.35, ch=0.012)
sh.box_yaw("cardboard", (0.36, 0.26, 0.3), (-0.35, 1.1, -0.05), -0.1, ch=0.012)
sh.box_yaw("tech_pants", (0.3, 0.24, 0.1), (0.6, 1.55, 0.0), 0.5, ch=0.008)  # registre
sh.export("shelf")

# carton isolé : boîte chanfreinée + couvercle + ruban
bx = Builder()
bx.chbox("cardboard", (0.7, 0.6, 0.7), (0, 0.3, 0), 0.02)
bx.chbox("cardboard", (0.74, 0.05, 0.74), (0, 0.62, 0), 0.01)
bx.box("receipt_paper", (0.2, 0.001, 0.14), (0.1, 0.646, 0.05))  # étiquette
bx.export("box_small")

# chemin de câbles : goulotte en U + 3 câbles + suspentes
ct = Builder()
ct.chbox("metal_dark", (2.0, 0.03, 0.3), (0, 0, 0), 0.006)
ct.box("metal_dark", (2.0, 0.07, 0.02), (0, -0.035, -0.14))
ct.box("metal_dark", (2.0, 0.07, 0.02), (0, -0.035, 0.14))
for zoff in (-0.07, 0.0, 0.07):                                # câbles
    ct.cyl("metal_dark", 0.018, 2.0, 6, (0, -0.075, zoff))
for sx in (-0.7, 0.7):                                         # suspentes
    ct.cyl("metal_dark", 0.008, 0.35, 4, (sx, 0.03, 0))
ct.export("cable_tray")

# gaine ventilation : caisson + brides + nervures
du = Builder()
du.chbox("duct", (2.0, 0.36, 0.5), (0, 0, 0), 0.015)
for sx in (-0.6, 0.6):
    du.box("metal_dark", (0.04, 0.4, 0.54), (sx, 0, 0))        # brides
du.box("metal_dark", (2.02, 0.06, 0.52), (0, 0.1, 0))          # bandeau
du.export("duct_seg")

# extincteur : corps + collerette + valve + poignée + tuyau
ex = Builder()
ex.cyl("extinguisher", 0.09, 0.5, 14, (0, 0.02, 0))
ex.cyl("extinguisher", 0.055, 0.05, 12, (0, 0.5, 0))           # collerette
ex.box("metal_dark", (0.05, 0.06, 0.08), (0, 0.56, 0))         # valve
ex.box("metal_dark", (0.16, 0.025, 0.05), (0.02, 0.6, 0))      # poignée
ex.cyl("metal_dark", 0.012, 0.2, 6, (0.08, 0.4, 0.02))         # tuyau
ex.export("extinguisher")

# coffret disjoncteur : caisson + porte + conduit + bande danger
br = Builder()
br.chbox("breaker_panel", (0.9, 1.3, 0.14), (0, 1.5, 0), 0.01)
br.box("hazard", (0.94, 0.08, 0.015), (0, 0.82, -0.075))
br.box("metal_dark", (0.08, 0.08, 0.14), (0.2, 2.18, 0))       # conduit montant
br.box("metal_dark", (0.02, 1.1, 0.02), (-0.43, 1.5, -0.08))   # charnière
br.export("breaker")

# terminal RH (kiosque) : corps, capot, écran en retrait, lecteur, clavier incliné
tm = Builder()
tm.chbox("terminal_body", (0.5, 1.05, 0.4), (0, 0.525, 0), 0.018)
tm.chbox("metal_dark", (0.54, 0.05, 0.44), (0, 1.07, 0.0), 0.01)
tm.chbox("metal_dark", (0.5, 0.42, 0.06), (0, 1.3, 0.14), 0.01)
tm.box("terminal_screen", (0.42, 0.32, 0.012), (0, 1.28, 0.176))
tm.chbox("terminal_body", (0.2, 0.05, 0.12), (0.3, 1.1, 0.14), 0.008)   # lecteur badge
tm.box("metal_dark", (0.4, 0.03, 0.16), (0, 1.0, 0.12))        # tablette clavier
tm.export("terminal")

# pile de justificatifs : 3 feuillets décalés + stylo
rc = Builder()
rc.box("receipt_paper", (0.16, 0.001, 0.22), (0, 0.011, 0))
rc.box_yaw("receipt_paper", (0.16, 0.001, 0.22), (0.05, 0.022, 0.03), 0.7)
rc.box_yaw("receipt_paper", (0.16, 0.001, 0.22), (-0.04, 0.033, -0.02), -0.45)
rc.cyl("metal_dark", 0.006, 0.14, 6, (0.1, 0.006, -0.09))      # stylo
rc.export("receipt")

# batterie : boîtier chanfreiné + capot + borne +
bt = Builder()
bt.chbox("battery", (0.12, 0.2, 0.06), (0, 0.1, 0), 0.006)
bt.box("metal_dark", (0.05, 0.015, 0.03), (0, 0.205, 0))       # borne
bt.export("battery")

# ---------------- personnages ----------------

# technicien (joueur distant) ~1.68 m : bottes, gilet à poches, badge,
# bras, tête + casque, sac à dos
tc = Builder()
for sx in (-0.11, 0.11):                                       # jambes + bottes
    tc.chbox("tech_pants", (0.15, 0.72, 0.17), (sx, 0.44, 0), 0.015)
    tc.chbox("metal_dark", (0.16, 0.09, 0.24), (sx, 0.045, 0.03), 0.012)
    tc.box_yaw("tech_pants", (0.15, 0.36, 0.16), (sx, 0.78, 0.0), sx * 0.04)  # cuisse
tc.chbox("tech_pants", (0.4, 0.14, 0.2), (0, 0.86, 0), 0.02)   # hanches
tc.chbox("tech_vest", (0.44, 0.52, 0.26), (0, 1.18, 0), 0.025) # torse
tc.chbox("tech_vest", (0.14, 0.12, 0.05), (-0.12, 1.1, -0.15), 0.01)    # poche
tc.chbox("tech_vest", (0.14, 0.12, 0.05), (0.12, 1.1, -0.15), 0.01)
tc.chbox("tech_pants", (0.3, 0.36, 0.12), (0, 1.2, 0.18), 0.015)        # sac à dos
tc.box("receipt_paper", (0.07, 0.001, 0.09), (0.14, 1.24, -0.145))      # badge
for sx in (-0.3, 0.3):                                         # bras (léger écart)
    tc.box_yaw("tech_vest", (0.1, 0.52, 0.13), (sx, 1.18, 0), sx * 0.1)
    tc.chbox("tech_head", (0.09, 0.1, 0.09), (sx * 1.18, 0.9, 0), 0.012) # mains
tc.chbox("tech_head", (0.24, 0.26, 0.24), (0, 1.56, 0), 0.03)  # tête
tc.chbox("tech_pants", (0.26, 0.06, 0.26), (0, 1.7, 0), 0.01)  # casquette
tc.box("tech_pants", (0.26, 0.02, 0.1), (0, 1.68, 0.16))       # visière
tc.box("metal_dark", (0.28, 0.03, 0.03), (0, 1.6, 0.02))       # arceau casque
for sx in (-0.13, 0.13):
    tc.chbox("metal_dark", (0.04, 0.06, 0.04), (sx, 1.58, 0.02), 0.008) # écouteurs
tc.export("tech")

# L'Auditeur : ~2.25 m, voûté, manteau, bras trop longs à griffes, masque pâle
en = Builder()
for sx in (-0.1, 0.1):                                         # jambes (2 segments)
    en.box_yaw("entity_cloth", (0.13, 0.6, 0.15), (sx, 0.36, 0), sx * 0.05)
    en.box_yaw("entity_cloth", (0.12, 0.62, 0.14), (sx * 1.25, 0.92, -0.02), -sx * 0.06)
    en.chbox("entity_cloth", (0.14, 0.06, 0.22), (sx * 1.15, 0.03, 0.04), 0.01)  # pied
en.chbox("entity_cloth", (0.46, 0.2, 0.24), (0, 1.06, 0), 0.02)          # bassin
en.chbox("entity_cloth", (0.5, 0.62, 0.27), (0, 1.45, -0.02), 0.025)     # torse voûté
en.box_yaw("entity_cloth", (0.46, 0.34, 0.26), (0, 1.75, -0.08), 0.0)    # emmanchures
for sx in (-0.34, 0.34):                                       # bras longs inclinés
    en.box_yaw("entity_cloth", (0.09, 0.7, 0.11), (sx, 1.45, -0.02), sx * 0.14)
    en.box_yaw("entity_cloth", (0.08, 0.6, 0.1), (sx * 1.35, 0.85, 0.05), sx * 0.05)
    for f in range(3):                                         # griffes
        en.box_yaw("entity_cloth", (0.02, 0.16, 0.02), (sx * (1.5 + f * 0.035), 0.52, 0.08), sx * 0.1)
en.chbox("entity_cloth", (0.5, 0.3, 0.24), (0, 1.14, 0.02), 0.02)        # queue de manteau
en.box_yaw("entity_mask", (0.28, 0.34, 0.26), (0, 2.0, 0.1), 0.0)        # masque penché
en.box("eyes", (0.05, 0.025, 0.02), (-0.065, 2.05, 0.235))               # yeux émissifs
en.box("eyes", (0.05, 0.025, 0.02), (0.065, 2.05, 0.235))
en.export("entity")

print("\nModèles générés dans", OUT)
