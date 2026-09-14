#!/usr/bin/env python3
"""Trouve les néons proches du spawn + le yaw pour les viser (test RT)."""
import re, math

src = open('/home/z/my-project/support-level-3/crates/shared/src/map.rs').read()
m = re.search(r'pub const MAP: &\[&str\] = &\[(.*?)\];', src, re.S)
assert m, "MAP introuvable"
rows = re.findall(r'"([^"]*)"', m.group(1))
rows = [r for r in rows if len(r) > 10]
print('rows', len(rows), 'cols', len(rows[0]))

Ls, S = [], None
for r, row in enumerate(rows):
    for c, ch in enumerate(row):
        if ch in 'Ll':
            Ls.append((c, r, ch))
        if ch in 'Ss' and S is None:
            S = (c, r)
print('spawn cell', S, '| nb lights', len(Ls))
sx, sr = S
for c, r, ch in sorted(Ls, key=lambda t: (t[0]-sx)**2 + (t[1]-sr)**2)[:8]:
    dx, dz = (c - sx) * 2.0, (r - sr) * 2.0
    d = math.hypot(dx, dz)
    # convention moteur : yaw += dx_mouse ; direction regard = (sin yaw, cos yaw)?
    print(f'light({c},{r}) "{ch}" d={d:.1f}m dx={dx} dz={dz}')
