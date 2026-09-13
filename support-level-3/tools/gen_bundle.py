#!/usr/bin/env python3
"""Génère crates/client/src/assets_bundle.rs : toutes les ressources du jeu
sont embarquées dans l'exécutable (include_bytes!) — l'exe est autoportant,
il se lance depuis n'importe quel dossier, sur n'importe quelle machine.

À relancer après toute modification du dossier assets/ (le test
`bundle_covers_disk_exactly` vérifie la cohérence bundle <-> disque).
"""

import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(ROOT, "assets")
OUT = os.path.join(ROOT, "crates", "client", "src", "assets_bundle.rs")


def main() -> int:
    if not os.path.isdir(ASSETS):
        print(f"erreur: {ASSETS} introuvable", file=sys.stderr)
        return 1

    entries = []
    for dirpath, dirnames, filenames in os.walk(ASSETS):
        dirnames.sort()
        for f in sorted(filenames):
            full = os.path.join(dirpath, f)
            rel = os.path.relpath(full, ASSETS).replace(os.sep, "/")
            entries.append(rel)

    if not entries:
        print("erreur: assets/ vide", file=sys.stderr)
        return 1

    lines = [
        "// Généré par tools/gen_bundle.py — NE PAS ÉDITER.",
        "// Toutes les ressources du jeu sont embarquées dans le binaire :",
        "// l'exécutable est autoportant (lancement depuis n'importe quel dossier).",
        "",
        "pub static BUNDLE: &[(&str, &[u8])] = &[",
    ]
    total = 0
    for rel in entries:
        size = os.path.getsize(os.path.join(ASSETS, rel))
        total += size
        lines.append(
            f'    ("{rel}", include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/{rel}"))),'
        )
    lines.append("];")
    lines.append("")

    with open(OUT, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(lines))

    print(f"assets_bundle.rs : {len(entries)} fichiers, {total/1e6:.1f} Mo embarqués")
    return 0


if __name__ == "__main__":
    sys.exit(main())
