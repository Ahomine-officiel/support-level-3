#!/usr/bin/env python3
"""Génère les modèles 3D du jeu au format GLTF 2.0 (.gltf + .bin).
Boîtes, cylindres et plans avec normales et UV corrects (winding CCW, main droite, Y up)."""
import json, math, os, struct

OUT = "/home/z/my-project/support-level-3/assets/models"
os.makedirs(OUT, exist_ok=True)

class Mesh:
    def __init__(self):
        self.pos = []    # (x,y,z)
        self.nrm = []    # (x,y,z)
        self.uv = []     # (u,v)
        self.idx = []    # indices
        self.prims = []  # (material, start, count) sur idx

    def add_tri(self, a, b, c, na, nb, nc, ua, ub, uc):
        s = len(self.pos)
        self.pos += [a, b, c]
        self.nrm += [na, nb, nc]
        self.uv += [ua, ub, uc]
        self.idx += [s, s + 1, s + 2]

    def add_quad(self, v0, v1, v2, v3, u0=(0, 0), u1=(1, 0), u2=(1, 1), u3=(0, 1)):
        s = len(self.pos)
        self.pos += [v0, v1, v2, v3]
        a = [v1[i] - v0[i] for i in range(3)]
        b = [v2[i] - v0[i] for i in range(3)]
        n = (a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0])
        l = math.sqrt(sum(x*x for x in n)) or 1.0
        n = (n[0]/l, n[1]/l, n[2]/l)
        self.nrm += [n]*4
        self.uv += [u0, u1, u2, u3]
        self.idx += [s, s+1, s+2, s, s+2, s+3]

def add_box(m, mat, size, t=(0, 0, 0), uv=None):
    """Boîte centrée sur size/2, translatée de t. uv : facteur global ou par face."""
    sx, sy, sz = (s * 0.5 for s in size)
    x, y, z = t
    def f(uv, dims):
        return (uv, uv) if isinstance(uv, (int, float)) else uv(dims)
    if uv is None:
        uv = lambda dims: (dims[0]/2.0, dims[1]/2.0)
    hw, hh, hd = size
    # +X
    d = (hd, hh)
    u = f(uv, d)
    m.add_quad((x+sx, y-sy, z+sz), (x+sx, y-sy, z-sz), (x+sx, y+sy, z-sz), (x+sx, y+sy, z+sz),
               (0, 0), (u[0], 0), u, (0, u[1]))
    # -X
    m.add_quad((x-sx, y-sy, z-sz), (x-sx, y-sy, z+sz), (x-sx, y+sy, z+sz), (x-sx, y+sy, z-sz),
               (0, 0), (u[0], 0), u, (0, u[1]))
    # +Y
    d = (hw, hd)
    u = f(uv, d)
    m.add_quad((x-sx, y+sy, z+sz), (x+sx, y+sy, z+sz), (x+sx, y+sy, z-sz), (x-sx, y+sy, z-sz),
               (0, 0), (u[0], 0), u, (0, u[1]))
    # -Y
    m.add_quad((x-sx, y-sy, z-sz), (x+sx, y-sy, z-sz), (x+sx, y-sy, z+sz), (x-sx, y-sy, z+sz),
               (0, 0), (u[0], 0), u, (0, u[1]))
    # +Z
    d = (hw, hh)
    u = f(uv, d)
    m.add_quad((x-sx, y-sy, z+sz), (x+sx, y-sy, z+sz), (x+sx, y+sy, z+sz), (x-sx, y+sy, z+sz),
               (0, 0), (u[0], 0), u, (0, u[1]))
    # -Z
    m.add_quad((x+sx, y-sy, z-sz), (x-sx, y-sy, z-sz), (x-sx, y+sy, z-sz), (x+sx, y+sy, z-sz),
               (0, 0), (u[0], 0), u, (0, u[1]))
    m.prims.append(mat)  # marquage : géré par add_group ci-dessous

def add_group(m, mat, start_vertex, count_idx):
    """Découpe les primitives par matériau : on empile les index par matériau."""
    m.prims.append(("GROUP", mat, start_vertex, count_idx))

class Builder:
    """Assemble plusieurs boîtes/cylindres, chaque ajout = un matériau (une primitive)."""
    def __init__(self):
        self.pos, self.nrm, self.uv, self.idx = [], [], [], []
        self.prim_ranges = []  # (mat, idx_start, idx_count)

    def box(self, mat, size, t=(0, 0, 0), uv=None):
        sub = Mesh()
        add_box(sub, mat, size, t, uv)
        self.merge(sub, mat)

    def cyl(self, mat, r, h, seg=16, t=(0, 0, 0), caps=True):
        cx, cy, cz = t
        base = len(self.pos)
        ring = []
        for i in range(seg):
            a = 2 * math.pi * i / seg
            ring.append((math.sin(a), math.cos(a)))
        # flanc
        s0 = len(self.idx)
        for i in range(seg):
            j = (i + 1) % seg
            ax, az = ring[i]
            bx, bz = ring[j]
            n = ((ax + bx) * 0.5, 0.0, (az + bz) * 0.5)
            l = math.hypot(n[0], n[2]) or 1.0
            n = (n[0]/l, n[1]/l, n[2]/l)
            v0 = (cx + ax*r, cy, cz + az*r)
            v1 = (cx + bx*r, cy, cz + bz*r)
            v2 = (cx + bx*r, cy + h, cz + bz*r)
            v3 = (cx + ax*r, cy + h, cz + az*r)
            s = len(self.pos)
            self.pos += [v0, v1, v2, v3]
            self.nrm += [n]*4
            self.uv += [(i/seg, 0), ((i+1)/seg, 0), ((i+1)/seg, 1), (i/seg, 1)]
            self.idx += [s, s+1, s+2, s, s+2, s+3]
        # caps
        if caps:
            for (yy, flip) in ((cy, False), (cy + h, True)):
                c = (cx, yy, cz)
                cs = len(self.pos)
                self.pos.append(c)
                self.nrm.append((0.0, 1.0 if yy > cy else -1.0, 0.0))
                self.uv.append((0.5, 0.5))
                for i in range(seg):
                    ax, az = ring[i]
                    bx, bz = ring[(i+1) % seg]
                    self.pos.append((cx + ax*r, yy, cz + az*r))
                    self.nrm.append((0.0, 1.0 if yy > cy else -1.0, 0.0))
                    self.uv.append((0.5 + ax*0.5, 0.5 + az*0.5))
                for i in range(seg):
                    a, b = cs + 1 + i, cs + 1 + (i+1) % seg
                    self.idx += ([cs, a, b] if not flip else [cs, b, a])
        self.prim_ranges.append((mat, s0, len(self.idx) - s0))

    def plane(self, mat, w, h, t=(0, 0, 0), axis="z", rot=0.0):
        """Plan. axis 'z' : vertical face +Z ; 'y' : horizontal face +Y (w=X, h=Z)."""
        cx, cy, cz = t
        if axis == "z":
            if rot:
                ca, sa = math.cos(rot), math.sin(rot)
                corners = [(-w/2, -h/2), (w/2, -h/2), (w/2, h/2), (-w/2, h/2)]
                pts = [(cx + u*ca - v*sa, cy + v, cz + u*sa + v*ca) for (u, v) in corners]
                self.add_quad_pts(pts, (0, 0, 1), [(0, 0), (1, 0), (1, 1), (0, 1)], mat)
            else:
                self.quad(mat,
                          (cx-w/2, cy-h/2, cz), (cx+w/2, cy-h/2, cz),
                          (cx+w/2, cy+h/2, cz), (cx-w/2, cy+h/2, cz),
                          [(0, 0), (1, 0), (1, 1), (0, 1)])
        else:
            self.quad(mat,
                      (cx-w/2, cy, cz-h/2), (cx+w/2, cy, cz-h/2),
                      (cx+w/2, cy, cz+h/2), (cx-w/2, cy, cz+h/2),
                      [(0, 0), (1, 0), (1, 1), (0, 1)])

    def quad(self, mat, v0, v1, v2, v3, uvs):
        sub = Mesh()
        sub.add_quad(v0, v1, v2, v3, uvs[0], uvs[1], uvs[2], uvs[3])
        self.merge(sub, mat)

    def add_quad_pts(self, pts, n, uvs, mat):
        s = len(self.pos)
        for p in pts:
            self.pos.append(p)
            self.nrm.append(n)
        self.uv += uvs
        i0 = len(self.idx)
        self.idx += [s, s+1, s+2, s, s+2, s+3]

    def merge(self, sub, mat):
        base_pos = len(self.pos)
        base_idx = len(self.idx)
        self.pos += sub.pos
        self.nrm += sub.nrm
        self.uv += sub.uv
        self.idx += [i + base_pos for i in sub.idx]
        self.prim_ranges.append((mat, base_idx, len(self.idx) - base_idx))

    def export(self, name):
        mats = []
        for (mat, _, _) in self.prim_ranges:
            if mat not in mats:
                mats.append(mat)
        prims = []
        for (mat, start, count) in self.prim_ranges:
            prims.append({
                "attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2},
                "indices": 3,
                "material": mats.index(mat),
            })
        # chaque primitive partage les accessors : on écrit tout en un seul gros buffer
        # et on découpe via des accessors PAR primitive (offsets)
        # -> plus simple : accessors uniques, primitives tronquées par ranges d'indices
        #    mais POSITION doit être par primitive… on exporte donc les accessors globaux
        #    et des primitives avec les mêmes accessors (attributs partagés autorisés).
        for p in self.prim_ranges:
            pass
        prim_list = []
        for k, (mat, start, count) in enumerate(self.prim_ranges):
            prim_list.append({
                "attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2},
                "indices": 3 + k,
                "material": mats.index(mat),
            })
        # buffers
        def pack_floats(vals, comps):
            out = b"".join(struct.pack("<%df" % comps, *v) for v in vals)
            return out
        pv = pack_floats(self.pos, 3)
        nv = pack_floats(self.nrm, 3)
        tv = pack_floats(self.uv, 2)
        iv = b"".join(struct.pack("<I", i) for i in self.idx)
        # accessors d'indices : un par primitive (subrange du même bufferView)
        views = []
        accs = []
        def add_view(data, target=None):
            while len(blob) % 4:
                blob.append(0)  # padding (muté via liste englobante)
            off = len(blob)
            blob.extend(data)
            v = {"buffer": 0, "byteOffset": off, "byteLength": len(data)}
            if target:
                v["target"] = target
            views.append(v)
            return len(views) - 1

        blob = bytearray()
        # positions
        v_p = add_view(pv, 34962)
        v_n = add_view(nv, 34962)
        v_t = add_view(tv, 34962)
        v_i = add_view(iv, 34963)
        def fmin(vals): return [min(v[i] for v in vals) for i in range(3)]
        def fmax(vals): return [max(v[i] for v in vals) for i in range(3)]
        accs.append({"bufferView": v_p, "componentType": 5126, "count": len(self.pos), "type": "VEC3", "min": fmin(self.pos), "max": fmax(self.pos)})
        accs.append({"bufferView": v_n, "componentType": 5126, "count": len(self.nrm), "type": "VEC3"})
        accs.append({"bufferView": v_t, "componentType": 5126, "count": len(self.uv), "type": "VEC2"})
        accs.append({"bufferView": v_i, "componentType": 5125, "count": len(self.idx), "type": "SCALAR"})
        # accessors d'indices par primitive
        idx_acc = []
        for (mat, start, count) in self.prim_ranges:
            byte_off = start * 4
            accs.append({"bufferView": v_i, "byteOffset": byte_off, "componentType": 5125, "count": count, "type": "SCALAR"})
            idx_acc.append(len(accs) - 1)
        prim_list = []
        for k, (mat, start, count) in enumerate(self.prim_ranges):
            prim_list.append({
                "attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2},
                "indices": idx_acc[k],
                "material": mats.index(mat),
            })
        gltf = {
            "asset": {"version": "2.0", "generator": "sl3-tools"},
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
        print(f"modèle: {name} ({len(self.pos)} sommets, {len(self.prim_ranges)} primitives, matériaux: {', '.join(mats)})")

# ---------------- Modèles ----------------
B = Builder()

# mur 2x3x0.24
w = Builder()
w.box("concrete", (2.0, 3.0, 0.24), (0, 1.5, 0), uv=lambda d: (d[0]/2.0, d[1]/2.0))
w.export("wall")

p = Builder()
p.box("concrete_dark", (0.5, 3.0, 0.5), (0, 1.5, 0))
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

# néon : cadre + panneau émissif
lf = Builder()
lf.box("metal_dark", (1.3, 0.06, 0.4), (0, -0.03, 0))
lf.box("light_panel", (1.2, 0.03, 0.3), (0, 0.0, 0))
lf.export("light_fixture")

# rack décoratif
rk = Builder()
rk.box("metal_dark", (1.9, 2.15, 0.75), (0, 1.075, 0))
rk.box("rack_front", (1.7, 1.9, 0.02), (0, 1.075, -0.39))
rk.export("rack")

# baie serveur interactive (LED = led_strip, teintée par instance)
sb = Builder()
sb.box("metal_dark", (1.5, 2.0, 0.8), (0, 1.0, 0))
sb.box("server_front", (1.3, 1.8, 0.03), (0, 1.0, -0.42))
sb.box("led_strip", (1.3, 0.05, 0.02), (0, 1.92, -0.435))
sb.export("server_bay")

# bureau complet : table + chaise + écran + clavier
ds = Builder()
ds.box("desk_wood", (1.6, 0.06, 0.8), (0, 0.75, -0.1))                    # plateau
for lx in (-0.72, 0.72):
    for lz in (-0.42, 0.22):
        ds.box("metal_dark", (0.06, 0.75, 0.06), (lx, 0.375, lz - 0.1))   # pieds
ds.box("chair_fabric", (0.45, 0.06, 0.45), (0, 0.48, 0.62))               # assise
ds.box("chair_fabric", (0.45, 0.52, 0.06), (0, 0.78, 0.84))               # dossier
ds.cyl("metal_dark", 0.24, 0.04, 12, (0, 0.02, 0.62))                     # base chaise
ds.cyl("metal_dark", 0.03, 0.42, 8, (0, 0.04, 0.62))                      # vérin
ds.box("metal_dark", (0.24, 0.02, 0.18), (0, 0.9, -0.28))                 # pied écran
ds.box("metal_dark", (0.05, 0.3, 0.05), (0, 1.04, -0.28))                 # col écran
ds.box("metal_dark", (0.58, 0.37, 0.03), (0, 1.24, -0.28))                # cadre écran
ds.box("screen_on", (0.54, 0.33, 0.01), (0, 1.24, -0.262))                # dalle émissive
ds.box("metal_dark", (0.42, 0.03, 0.15), (0, 0.79, 0.05))                 # clavier
ds.export("desk_set")

# porte : dormant + panneau coulissant séparé
df = Builder()
df.box("door_frame", (0.12, 2.2, 0.16), (-0.62, 1.1, 0))
df.box("door_frame", (0.12, 2.2, 0.16), (0.62, 1.1, 0))
df.box("door_frame", (1.36, 0.12, 0.16), (0, 2.26, 0))
df.export("door_frame")
dp = Builder()
dp.box("door_metal", (1.12, 2.2, 0.08), (0, 1.1, 0))
dp.export("door_panel")

# porte de sortie (hall) + panneau coulissant vertical + panneau émissif
ed = Builder()
ed.box("door_frame", (0.14, 2.5, 0.2), (-3.5, 1.25, 0))
ed.box("door_frame", (0.14, 2.5, 0.2), (3.5, 1.25, 0))
ed.box("door_frame", (7.14, 0.16, 0.2), (0, 2.58, 0))
ed.box("exit_sign", (1.3, 0.42, 0.06), (0, 2.95, 0.05))
ed.export("exit_door")
ep = Builder()
ep.box("door_metal", (3.3, 2.5, 0.1), (-1.72, 1.25, 0))
ep.box("door_metal", (3.3, 2.5, 0.1), (1.72, 1.25, 0))
ep.export("exit_panel")

# rayonnage d'archives
sh = Builder()
sh.box("shelf_metal", (0.05, 2.0, 0.5), (-0.95, 1.0, 0))
sh.box("shelf_metal", (0.05, 2.0, 0.5), (0.95, 1.0, 0))
for y in (0.3, 0.85, 1.4, 1.95):
    sh.box("shelf_metal", (1.9, 0.04, 0.5), (0, y, 0))
sh.box("cardboard", (0.5, 0.35, 0.4), (-0.5, 0.5, 0))
sh.box("cardboard", (0.45, 0.3, 0.35), (0.4, 1.05, 0.02))
sh.box("cardboard", (0.4, 0.28, 0.3), (0.1, 1.6, -0.02))
sh.export("shelf")

bx = Builder()
bx.box("cardboard", (0.7, 0.6, 0.7), (0, 0.3, 0))
bx.box("cardboard", (0.74, 0.05, 0.74), (0, 0.62, 0))
bx.export("box_small")

# chemin de câbles (plafond) + gaine ventilation
ct = Builder()
ct.box("metal_dark", (2.0, 0.05, 0.28), (0, 0, 0))
for zoff in (-0.06, 0.0, 0.06):
    ct.cyl("metal_dark", 0.018, 2.0, 6, (0, -0.09, zoff))
ct.export("cable_tray")

du = Builder()
du.box("duct", (2.0, 0.36, 0.5), (0, 0, 0))
du.box("metal_dark", (2.02, 0.4, 0.04), (0, 0, 0))
du.export("duct_seg")

# extincteur
ex = Builder()
ex.cyl("extinguisher", 0.09, 0.5, 12, (0, 0, 0))
ex.box("metal_dark", (0.16, 0.03, 0.05), (0, 0.52, 0))
ex.export("extinguisher")

# coffret disjoncteur (mur)
br = Builder()
br.box("breaker_panel", (0.9, 1.3, 0.16), (0, 1.5, 0))
br.box("hazard", (0.94, 0.08, 0.17), (0, 0.82, 0))
br.export("breaker")

# terminal RH (kiosque)
tm = Builder()
tm.box("terminal_body", (0.5, 1.05, 0.4), (0, 0.525, 0))
tm.box("metal_dark", (0.5, 0.04, 0.5), (0, 1.07, 0.02))
tm.box("metal_dark", (0.46, 0.36, 0.04), (0, 1.28, 0.16))
tm.box("terminal_screen", (0.42, 0.32, 0.01), (0, 1.28, 0.185))
tm.export("terminal")

# pile de justificatifs (3 feuillets)
rc = Builder()
rc.plane("receipt_paper", 0.16, 0.22, (0, 0.012, 0), axis="y")
rc.plane("receipt_paper", 0.16, 0.22, (0.05, 0.024, 0.03), axis="y", rot=0.7)
rc.plane("receipt_paper", 0.16, 0.22, (-0.04, 0.036, -0.02), axis="y", rot=-0.45)
rc.export("receipt")

# batterie
bt = Builder()
bt.box("battery", (0.12, 0.2, 0.06), (0, 0.1, 0))
bt.export("battery")

# technicien (joueur distant) : gilet orange, ~1.65 m
tc = Builder()
tc.box("tech_pants", (0.16, 0.8, 0.18), (-0.11, 0.4, 0))
tc.box("tech_pants", (0.16, 0.8, 0.18), (0.11, 0.4, 0))
tc.box("tech_vest", (0.44, 0.55, 0.24), (0, 1.08, 0))
tc.box("tech_vest", (0.11, 0.6, 0.14), (-0.3, 1.05, 0))
tc.box("tech_vest", (0.11, 0.6, 0.14), (0.3, 1.05, 0))
tc.box("tech_head", (0.24, 0.26, 0.24), (0, 1.51, 0))
tc.export("tech")

# L'Auditeur : silhouette 2.2 m, bras longs, masque pâle, yeux émissifs
en = Builder()
en.box("entity_cloth", (0.14, 1.1, 0.16), (-0.1, 0.55, 0))
en.box("entity_cloth", (0.14, 1.1, 0.16), (0.1, 0.55, 0))
en.box("entity_cloth", (0.5, 0.75, 0.26), (0, 1.47, 0))
en.box("entity_cloth", (0.1, 1.05, 0.12), (-0.33, 1.32, 0))
en.box("entity_cloth", (0.1, 1.05, 0.12), (0.33, 1.32, 0))
en.box("entity_mask", (0.3, 0.34, 0.28), (0, 2.02, 0))
en.box("eyes", (0.05, 0.025, 0.02), (-0.07, 2.06, 0.145))
en.box("eyes", (0.05, 0.025, 0.02), (0.07, 2.06, 0.145))
en.export("entity")

print("\nModèles générés dans", OUT)
