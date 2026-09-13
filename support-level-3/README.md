# SUPPORT LEVEL -3 🎮👻

> **FR** — Un jeu d'horreur coopératif en ligne (1 à 4 joueurs) sur le thème du **support informatique**, écrit en **Rust** avec le moteur de rendu **wgpu**. Vous êtes technicien de nuit au sous-sol niveau -3 : réparez les baies serveurs, faites valider votre note de frais… et échappez à **L'Auditeur**.
>
> **EN** — An online co-op horror game (1-4 players) themed around **IT support**, written in **Rust** with the **wgpu** rendering engine. You are the night-shift technician in basement level -3: fix the server racks, get your expense report approved… and escape **The Auditor**.

---

## 🇫🇷 Le pitch

Minuit. Le niveau -3 du siège. Le ticketing déborde et **six baies serveurs sont tombées en même temps**. Votre mission de nuit :

1. **Réparer les 6 serveurs** (maintenir `E` pour rebooter — ça fait du bruit).
2. **Récupérer 4 justificatifs** éparpillés (ticket resto, taxi, matériel, péage).
3. **Faire valider la note de frais** au terminal RH : **42,73 € remboursés**.
4. Les deux objectifs remplis → la **porte de secours** du hall s'ouvre. Fuyez.

Le problème : un ancien « ticket non résolu » hante les couloirs. **L'Auditeur** patrouille, entend vos pas (la course attire !), entend vos reboots, ouvre les portes et ne lâche prise. Attrapé, vous êtes au sol : un coéquipier peut vous relever en maintenant `E`… si vous tenez 45 secondes.

**Coupures de courant aléatoires**, néons qui clignotent, lampe torche à batterie limitée, vision qui se déforme quand il approche.

## 🇬🇧 The pitch

Midnight. Headquarters, level -3. Six server racks have crashed at once. Your night shift:

1. **Repair the 6 servers** (hold `E` to reboot — it makes noise).
2. **Collect 4 receipts** scattered around (lunch voucher, taxi, hardware, toll).
3. **Get the expense report approved** at the HR terminal: **€42.73 reimbursed**.
4. Both objectives done → the **emergency exit** unlocks. Run.

The catch: an old "unresolved ticket" haunts the halls. **The Auditor** patrols, hears your footsteps (sprinting attracts!), hears your reboots, opens doors and never gives up. Caught, you go down: a teammate can revive you by holding `E`… if you last 45 seconds.

**Random blackouts**, flickering fluorescents, limited flashlight battery, vision that warps as he approaches.

---

## 🛠️ Build

Prérequis : **Rust stable** (https://rustup.rs) — testé avec 1.98.

```bash
git clone <ce dépôt> && cd support-level-3
cargo build --release
```

Binaires produites : `target/release/sl3-client` (le jeu) et `target/release/sl3-server` (serveur dédié).

**Audio** : le client utilise `rodio`/`cpal`.
- **Windows / macOS** : rien à installer.
- **Linux** : `sudo apt install libasound2-dev` (ou l'équivalent de votre distribution). Sans ALSA : `cargo build --release -p sl3-client --no-default-features` (jeu muet).

**Lancement** : l'exécutable est **autoportant** — toutes les ressources (modèles, textures, sons, police) sont **embarquées dans le binaire**. Lancez-le depuis n'importe quel dossier, copiez-le seul, déplacez-le : aucune dépendance à `assets/` (la config `sl3_config.json` est créée à côté de l'exe).

```bash
./target/release/sl3-server --port 27070     # terminal 1 : le serveur
./target/release/sl3-client                  # terminal 2 : le jeu (fonctionne d'où vous voulez)
```

## 🌐 Jouer en ligne (rooms)

Le client se connecte toujours à une **adresse de serveur** (par défaut `127.0.0.1:27070`). Le serveur gère plusieurs **rooms** simultanées, avec ou sans mot de passe.

1. Une personne lance `sl3-server` sur une machine joignable :
   - **LAN** : rien à faire, l'IP locale suffit.
   - **Internet** : serveur dédié/VPS, ou redirection de port du routeur vers le port 27070 (TCP).
2. Les joueurs lancent `sl3-client` :
   - `[1] Héberger` → adresse du serveur → mot de passe (vide = aucun) → un **code de room à 4 caractères** est généré.
   - `[2] Rejoindre` → adresse → **code** → mot de passe.
3. L'hôte presse `[Espace]` pour démarrer la partie.

Test rapide sans GUI : `./target/release/sl3-server --bots 2` démarre une room avec 2 bots qui errent et réparent.

**[F12] en jeu** : sauvegarde une capture d'écran PNG à côté de l'exécutable.

**Régénération des assets** : après modification de `assets/`, relancez `python3 tools/gen_bundle.py` pour rembarquer les fichiers dans le binaire (le test `bundle_covers_disk_exactly` vérifie la cohérence).

**Mode autopilot (dev/QA)** : `SL3_AUTOPILOT="key Digit1@1; click 640 420@2; shot /tmp/x.png@3; mouse 640 300@3.5; yaw 90@4; exit@5" ./sl3-client` pilote le jeu (touches, **clics souris sur les boutons**, déplacement du curseur, yaw caméra) et capture des écrans automatiquement — c'est ce qui a permis de valider visuellement le rendu (menu cliquable, options FSR 3/DLSS, lobby, HUD, RT) sur pilote logiciel. `SL3_WINDOW_SIZE=640x360` réduit la fenêtre (tests sur pilote logiciel), `SL3_DEBUG=1` trace les frames et les clics.

## 🎮 Commandes

| Touche | Action |
|---|---|
| `Z Q S D` / `W A S D` | Se déplacer |
| Souris | Regarder |
| `Maj` | Courir (endurance limitée — et ça attire l'Auditeur) |
| `E` | Interagir (maintenir : reboot, terminal, disjoncteur, relever) |
| `F` | Lampe torche (batterie !) |
| `F5` | Ray tracing : Off → Qualité → Ultra (en jeu) |
| `F6` | Upscaling : Natif → FSR 3 → DLSS (en jeu) |
| `F12` | Capture d'écran PNG (à côté de l'exe) |
| `Échap` | Pause / retour |
| `F1` | Basculer **FR ⇄ EN** |
| Souris (menus) | **Tous les menus sont cliquables** : boutons, survol, options en un clic |

## 🚀 Upscaling FSR 3 / DLSS (boutons dans Options)

Le client embarque un **upscaling temporel maison — port WGSL du noyau FSR 3**
(FidelityFX Super Resolution 2/3 d'AMD, MIT) : reconstruction plein écran à
partir d'un rendu interne réduit, avec **vecteurs de mouvement** (reprojection
de la profondeur), **dilatation** (voisin le plus proche), **Lanczos 2 à biais
de noyau**, **boîte de rectification** (variance YCoCg), **reprojection de
l'historique** (Lanczos 4×4) et **RCAS** (netteté FSR en espace perceptuel).
Jitter Halton(2,3) sur 8 phases appliqué à la projection.

Dans **Options** (boutons cliquables) :

| Bouton | Rôle |
|---|---|
| **Natif** | rendu pleine résolution classique (défaut) |
| **FSR 3** | upscaling temporel sur tous les GPU — interne ×1.5 / ×1.7 / ×2.0 selon le preset |
| **DLSS** | même moteur temporel, bouton **activé sur RTX uniquement** (grisé « RTX requis » ailleurs) ; le SDK NVIDIA fermé ne peut pas être intégré, nous fournissons notre reconstruction du même type |
| **Qualité d'upscaling** | Qualité (×1.5) / Équilibré (×1.7) / Performance (×2.0) |

- La ligne perf affiche `– FSR 3 Qualité` (ou DLSS) quand c'est actif.
- **L'UI reste toujours en pleine résolution** (menus, HUD, textes nets).
- Le preset se **combine à l'échelle de rendu** (DRS) : échelle interne =
  DRS × preset — l'auto-DRS continue de protéger le framerate.
- `F6` en jeu cycle Natif → FSR 3 → (DLSS si RTX), avec toast dans le journal.
- L'historique temporel est invalidé proprement (redimensionnement, changement
  de preset, début de partie, téléport).
- Limites assumées : pas de masques réactifs/transparence ni depth-clip
  (scatter) du SDK complet — inutiles pour cette scène LDR ; la génération
  d'images (frame interpolation) n'est pas incluse.

## 🖥️ Performances (vieux PC / iGPU)

Le client intègre une **résolution dynamique** : le monde est rendu dans un buffer
à échelle réduite puis l'upscale passe par le post-process (UI toujours nette en
pleine résolution). Par défaut en mode **auto** — l'échelle s'ajuste toutes les
0,75 s pour viser ~60 fps (plancher 45 %). La ligne en haut à droite affiche
`FPS · rendu XX%`.

Dans **Options**, `R` cycle le mode : `auto → 100 % → 85 % → 70 % → 55 % → auto`
(choix mémorisé dans `sl3_config.json`). Côté assets : mipmaps générés à la main
(filtre box en espace linéaire) + filtrage anisotrope ×8, textures des grandes
surfaces sans couture en 512 px — peu d'aliasing, moins de bande passante.
Un i7 de 7e génération (iGPU HD 630) tourne confortablement à 1080p en laissant
l'auto à 60-75 %.

## ✨ Ray tracing (optionnel, RTX 2060+)

Le client propose un **vrai ray tracing par rayons** (ombres, occlusion ambiante,
rebond de lumière), calculé en shader contre une scène simplifiée en boîtes
(AABB) — ~80 boîtes statiques fusionnées + les objets dynamiques (portes,
baies serveurs, joueurs, l'Auditeur). wgpu 22 n'expose pas les RT cores (DXR),
les rayons tournent donc sur les unités de calcul : très rapide sur une RTX 2060
(~10 % du GPU en 1080p), et **strictement hors du chemin de rendu quand c'est
désactivé**.

Trois modes (mémorisés dans `sl3_config.json`) :

| Mode | Effets | Coût |
|---|---|---|
| **Off** (défaut) | rendu classique identique à la v2 | zéro |
| **Qualité** | ombres douces des néons (pénombres stables) + ombre de la torche + occlusion ambiante | tampon RT à 40 % de la résolution |
| **Ultra** | + un rebond de lumière (GI approximatif) teinté par les néons | tampon RT à 50 % |

- **En jeu** : `F5` cycle les modes (message dans le journal) — la ligne perf
  affiche `· RT` quand c'est actif.
- **Menu** : Options → `[T] Ray tracing`.
- **Auto-détection** : au premier lancement sur un GPU **RTX**, le mode
  *Qualité* est activé automatiquement.
- Le tampon RT suit l'échelle de rendu dynamique : si le DRS baisse,
  la passe RT baisse avec — pas de surprise sur la courbe de FPS.
- Sur une vieille machine (iGPU), laissez simplement **Off** : aucune texture,
  aucune passe, aucun octet de plus.

Pipelines impliqués : `prepass-depth` (profondeur vertex-seul) → `rt-pass`
(rayons : ombres/AO/GI, MRT rgba16f) → passe monde qui lit le résultat RT
(textures 1×1 neutres quand le RT est Off → image identique).

## 🧪 Tests

```bash
cargo test
```

- Tests unitaires **shared** : carte ASCII (connectivité BFS de tous les objectifs, longueur des lignes), roundtrip du protocole.
- Tests unitaires **server** : reboot complet d'un serveur, validation de la note de frais → ouverture de la sortie → évasion → fin de partie, audition de l'entité.
- Test d'intégration **lobby** : lance le vrai binaire serveur et joue toute la séquence en TCP (Hello → CreateRoom → mauvais mot de passe → JoinRoom → StartGame → GameStarted → snapshots → LeaveRoom).
- Test **assets** (client) : chaque GLTF charge via le même parseur que le jeu, matériaux connus du moteur, indices dans les bornes, budget de triangles respecté.
- Tests **shaders** (client) : les 5 shaders WGSL (world, post, ui, rt, upscale) sont parsés et validés par **naga** — erreurs de GPU détectées sans GPU.
- Tests **rtscene** (client) : fusion des murs (couverture de chaque cellule `#`), budget de boîtes, AABB tournées, cohérence de la scène statique.

## 🏗️ Architecture

```
support-level-3/
├── crates/
│   ├── shared/     Carte ASCII 48×30 (cellule = 2 m), protocole réseau (bincode
│   │               + frames length-prefixed sur TCP), constantes de jeu, textes FR/EN.
│   ├── server/     Serveur dédié multi-rooms (thread par room, tick 20 Hz) :
│   │               lobby avec codes + mots de passe, simulation autoritaire
│   │               (interactions, serveurs, justificatifs, blackouts),
│   │               IA de l'Auditeur (BFS sur la grille, ouïe, vision, poursuite,
│   │               capture, cooldown), bots de smoke test.
│   └── client/     Moteur wgpu 22 + winit 0.30 :
│                   • pipeline monde (instances, néons, lampe torche spot, brouillard)
│                   • ray tracing optionnel : pré-pass profondeur + passe de
│                     rayons (ombres douces, AO, GI) contre une scène AABB
│                     fusionnée (rt.wgsl + rtscene.rs), 3 modes Off/Qualité/Ultra
│                   • post-process « vision de panique » (distorsion, grain, vignette)
│                   • résolution dynamique (offscreen ×échelle, upscale post, UI 1:1)
│                   • mipmaps CPU (espace linéaire) + anisotropie ×8
│                   • UI bitmap bilingue (atlas de police généré)
│                   • chargement GLTF (géométrie/normales/UV) + textures PNG
│                   • audio rodio (ambiance, cœur, murmures, SFX spatialisés en gain)
│                   • FPS local prédictif + interpolation des entités réseau
├── assets/         Modèles .gltf+.bin (28), textures .png (40), sons .wav (21), police.
├── tools/          Générateurs Python des assets (numpy/PIL) :
│                   gen_map.py (carte + validation), gen_models.py (GLTF),
│                   gen_textures.py, gen_audio.py, gen_font.py.
└── Cargo.toml      Workspace (release : opt-level 2, LTO thin).
```

**Netcode** : TCP fiable, frames `u32 LE + bincode`, client autoritaire sur sa position (coop, pas de anti-cheat), serveur autoritaire sur l'IA et les objectifs, snapshots 20 Hz, interpolation exponentielle côté client.

**Boucle de victoire** : 6/6 serveurs en ligne **et** note de frais validée → sortie ouverte → tous les survivants dans la zone `X` du hall. Défaite si tout le monde est « audité ».

## 🔧 Régénérer les assets

```bash
python3 tools/gen_map.py        # régénère et valide la carte (injecter via tools/inject_map.py)
python3 tools/gen_models.py     # les 28 modèles GLTF
python3 tools/gen_textures.py   # les 40 textures
python3 tools/gen_audio.py      # les 21 sons
python3 tools/gen_font.py       # l'atlas de police (accents FR inclus)
```

## 📝 Notes de conception / limites connues

- Prototype coop : pas de revivals après expulsion, pas de chat texte (journal d'événements uniquement).
- Le ray tracing est une approximation géométrique (AABB) : les pénombres sont
  correctes aux portes/murs/meubles, les petits objets (cartons, extincteurs)
  ne projettent pas d'ombre.
- L'IA de l'Auditeur est déterministe en grille (BFS) : rapide et robuste, sans pathfinding hiérarchique.
- Le client fait confiance à la position annoncée des autres joueurs : assumé pour une coop entre amis.

*Niveau -3 — « Clause 7.3 : tout ticket non résolu au bout de 40 ans devient une personne. »*
