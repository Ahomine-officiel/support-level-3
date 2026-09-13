#!/usr/bin/env python3
"""Injecte la carte générée dans crates/shared/src/map.rs entre les marqueurs MAP: &[&str] = &[ et ];"""
import subprocess, re

out = subprocess.run(["python3", "tools/gen_map.py"], capture_output=True, text=True, cwd="/home/z/my-project/support-level-3")
lines = out.stdout.splitlines()
start = lines.index('"################################################",') if '"################################################",' in lines else None
rows = [l for l in lines if l.startswith('"')]
assert len(rows) == 30, len(rows)

path = "/home/z/my-project/support-level-3/crates/shared/src/map.rs"
src = open(path).read()
new_block = "pub const MAP: &[&str] = &[\n" + "\n".join(rows) + "\n];"
src = re.sub(r"pub const MAP: &\[&str\] = &\[.*?\];", new_block, src, flags=re.S)
open(path, "w").write(src)
# vérif longueurs
for r in rows:
    inner = r[1:-2]
    assert len(inner) == 48, (len(inner), r)
print("MAP injectée, 30 lignes x 48 OK")
