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

**Lancement** : exécutez depuis la racine du projet (le client charge `assets/` relativement au répertoire courant) :

```bash
./target/release/sl3-server --port 27070     # terminal 1 : le serveur
./target/release/sl3-client                  # terminal 2 : le jeu
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

## 🎮 Commandes

| Touche | Action |
|---|---|
| `Z Q S D` / `W A S D` | Se déplacer |
| Souris | Regarder |
| `Maj` | Courir (endurance limitée — et ça attire l'Auditeur) |
| `E` | Interagir (maintenir : reboot, terminal, disjoncteur, relever) |
| `F` | Lampe torche (batterie !) |
| `Échap` | Pause / retour |
| `F1` | Basculer **FR ⇄ EN** |

## 🧪 Tests

```bash
cargo test
```

- Tests unitaires **shared** : carte ASCII (connectivité BFS de tous les objectifs, longueur des lignes), roundtrip du protocole.
- Tests unitaires **server** : reboot complet d'un serveur, validation de la note de frais → ouverture de la sortie → évasion → fin de partie, audition de l'entité.
- Test d'intégration **lobby** : lance le vrai binaire serveur et joue toute la séquence en TCP (Hello → CreateRoom → mauvais mot de passe → JoinRoom → StartGame → GameStarted → snapshots → LeaveRoom).

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
│                   • post-process « vision de panique » (distorsion, grain, vignette)
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
- Pas de mipmaps (le brouillard masque l'aliasing lointain) — amélioration future : blit pipeline.
- L'IA de l'Auditeur est déterministe en grille (BFS) : rapide et robuste, sans pathfinding hiérarchique.
- Le client fait confiance à la position annoncée des autres joueurs : assumé pour une coop entre amis.

*Niveau -3 — « Clause 7.3 : tout ticket non résolu au bout de 40 ans devient une personne. »*
