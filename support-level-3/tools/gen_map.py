#!/usr/bin/env python3
"""Construit et valide la carte SL3 (48x30), puis imprime les lignes Rust."""
W, H = 48, 30
grid = [['#'] * W for _ in range(H)]

def fill(c0, r0, c1, r1, ch):
    for r in range(r0, r1 + 1):
        for c in range(c0, c1 + 1):
            grid[r][c] = ch

def put(c, r, ch):
    grid[r][c] = ch

# --- Pièces ---
fill(2, 1, 12, 5, '_')      # hall (nord-ouest)
fill(33, 1, 45, 5, ',')     # bureau du manager (nord-est)
fill(2, 7, 45, 8, '.')      # couloir nord
fill(15, 9, 16, 20, '.')    # couloir ouest (vertical)
fill(27, 9, 28, 20, '.')    # couloir central (vertical)
fill(2, 10, 13, 17, ',')    # open-space (ouest)
fill(18, 10, 25, 17, ',')   # salle de pause (centre)
fill(30, 10, 45, 19, ':')   # salle serveurs (est)
fill(2, 21, 45, 22, '.')    # couloir sud
fill(2, 24, 16, 28, ';')    # archives (sud-ouest)
fill(33, 24, 42, 28, '=')   # local électrique (sud-est)

# --- Sortie + spawns ---
for c in range(5, 9): put(c, 1, 'X')
put(7, 4, 'P')
put(28, 13, 'E')

# --- Portes ---
put(7, 6, 'D')    # hall -> couloir nord
put(39, 6, 'D')   # manager -> couloir nord
put(7, 9, 'D')    # open-space -> couloir nord
put(21, 9, 'D')   # pause -> couloir nord
put(37, 9, 'D')   # serveurs -> couloir nord
put(37, 20, 'D')  # serveurs -> couloir sud
put(8, 23, 'D')   # archives -> couloir sud
put(37, 23, 'D')  # électrique -> couloir sud

# --- Baies serveurs 0..5 (col 44, rangées 11-16) ---
for i, r in enumerate(range(11, 17)):
    put(44, r, str(i))

# --- Racks décoratifs (allées centrales de la salle serveurs) ---
for r in (12, 14, 16):
    for c in range(31, 42, 2):
        put(c, r, 'r')

# --- Open-space : desks ---
for (c, r) in [(4, 11), (8, 11), (12, 11), (4, 13), (8, 13), (12, 13)]:
    put(c, r, 'd')
put(6, 15, 'R')
put(7, 17, 'l')

# --- Bureau manager ---
put(33, 2, 'd'); put(35, 2, 'R'); put(40, 2, 'T'); put(42, 2, 'L'); put(44, 2, 'o')
put(40, 4, 'd')

# --- Salle de pause ---
put(20, 11, 'o'); put(21, 13, 'l'); put(22, 14, 'b')

# --- Salle serveurs : lumières, batterie ---
put(32, 10, 'L'); put(38, 10, 'L')
put(32, 19, 'L'); put(38, 19, 'L')
put(33, 17, 'b')

# --- Hall ---
put(3, 2, 'L'); put(11, 2, 'L'); put(3, 5, 'L'); put(10, 3, 'b')

# --- Couloirs ---
for (c, r, ch) in [(5,7,'L'), (24,7,'l'), (33,7,'L'), (43,7,'L'),
                   (16,8,'p'), (27,8,'p'),
                   (5,21,'L'), (14,21,'l'), (24,21,'L'), (33,21,'l'), (43,21,'L')]:
    put(c, r, ch)

# --- Archives ---
for r in (25, 27):
    for c in range(3, 14, 2):
        put(c, r, 'A')
put(4, 24, 'o'); put(10, 24, 'o')
put(5, 26, 'R'); put(13, 26, 'L'); put(14, 27, 'R'); put(4, 28, 'b')

# --- Local électrique ---
put(37, 24, 'l'); put(37, 25, 'B'); put(39, 26, 'l'); put(41, 27, 'b')

# --- Validation ---
errors = []
# longueurs
for r, row in enumerate(grid):
    assert len(row) == W, f"ligne {r}: {len(row)}"
# portes bien orientées
for (c, r) in [(7,6),(39,6),(7,9),(21,9),(37,9),(37,20),(8,23),(37,23)]:
    lr = grid[r][c-1] == '#' and grid[r][c+1] == '#'
    ud = grid[r-1][c] == '#' and grid[r+1][c] == '#'
    if not (lr != ud):
        errors.append(f"porte ({c},{r}) orientation invalide l/r={lr} u/d={ud}")
# connectivité BFS depuis P
from collections import deque
start = (7, 4)
targets = {}
for r in range(H):
    for c in range(W):
        ch = grid[r][c]
        if ch in '012345RTXb' or ch == 'E':
            targets[ch if ch in '012345' else ch] = (c, r)
walkable = lambda c, r: grid[r][c] not in '#Aado pr'.replace(' ','') + 'p'
def walk(c, r):
    ch = grid[r][c]
    return ch not in '#Adorp'
seen = {start}
q = deque([start])
while q:
    c, r = q.popleft()
    for dc, dr in ((1,0),(-1,0),(0,1),(0,-1)):
        nc, nr = c+dc, r+dr
        if 0 <= nc < W and 0 <= nr < H and (nc,nr) not in seen and walk(nc, nr):
            seen.add((nc, nr))
            q.append((nc, nr))
for name, (c, r) in sorted(targets.items()):
    if (c, r) not in seen:
        errors.append(f"cible '{name}' ({c},{r}) non accessible depuis P")
# 4 justificatifs, 5 batteries, 6 serveurs
n_r = sum(row.count('R') for row in grid)
n_b = sum(row.count('b') for row in grid)
n_s = sum(sum(1 for ch in row if ch in '012345') for row in grid)
assert n_r == 4, f"R={n_r}"
assert n_b == 5, f"b={n_b}"
assert n_s == 6, f"servers={n_s}"

print("ERREURS:", errors if errors else "aucune ✓")
print(f"cellules praticables: {len(seen)}")
print()
for row in grid:
    print('"' + ''.join(row) + '",')
