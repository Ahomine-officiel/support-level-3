# Worklog

---
Task ID: 1
Agent: Super Z (agent principal)
Task: Développer « SUPPORT LEVEL -3 » — jeu d'horreur coop multijoueur en Rust + wgpu, thème support informatique, modèles 3D GLTF générés, textures, audio procédural, rooms en ligne avec mot de passe, objectif « remboursement », UI bilingue FR/EN.

Work Log:
- Clarifications utilisateur (AskUserQuestion) : coop survie, rooms en ligne, GLTF générés, sous-sol mixte, tickets incident, toutes les mécaniques d'horreur (entité, torche, audio, vision de panique), bilingue, compilation exigée. Remarque : ajouter l'objectif « remboursement » et de vraies textures.
- Environnement : installation de Rust stable 1.98.1 (rustup). ALSA/Vulkan absents du conteneur → audio en feature optionnelle + tests logique/serveur au lieu de rendu headless.
- Carte ASCII 48×30 « sous-sol niveau -3 » construite et validée programmatiquement (tools/gen_map.py) : 753 cellules praticables, 8 portes orientées, BFS depuis le spawn atteint les 6 serveurs, 4 justificatifs, terminal RH, disjoncteur, sortie, spawn entité. Injection dans crates/shared/src/map.rs via tools/inject_map.py.
- Assets générés par scripts Python (tools/) : 28 modèles GLTF+bin (murs, sols×6, plafond, néons, racks, baies serveurs, bureaux+écrans, portes, rayonnages, cartons, goulottes de câbles, gaines, extincteur, disjoncteur, terminal RH, justificatifs, batteries, technicien, « L'Auditeur »), 40 textures PNG (béton, lino, métal, écrans, affiches humoristiques bilingues…), 21 sons WAV synthétisés (ambiance, buzz, battements, jumpscare, screech, pas, portes, blackouts…), atlas de police bitmap 136 glyphes (accents français).
- Crate shared : constantes de gameplay, carte + BFS/LOS, protocole TCP (frames u32 LE + bincode), textes bilingues (tickets d'incident, justificatifs, annonces).
- Crate server : serveur dédié multi-rooms (thread par room, tick 20 Hz), lobby avec codes 4 caractères + mots de passe, simulation autoritaire (inputs, interactions maintenues, serveurs 3 états, justificatifs, batteries, portes, terminal RH, disjoncteur, blackouts aléatoires, bleed-out, évasion, victoire/défaite), IA de l'Auditeur (patrouille BFS, ouïe, vision avec FOV+LOS, poursuite, capture → downed + cooldown + téléport, ouverture de portes, murmures), bots de smoke test avec pathfinding.
- Crate client : moteur wgpu 22.1 (pipeline monde avec 24 lumières + spot torche + émissifs par instance + brouillard, post-process « vision de panique » avec distorsion/aberration/grain/vignette/rouge downed, UI bitmap bilingue), FPS (ZQSD/WASD, souris recentrée, sprint/endurance, collision cercle/grille + portes fermées), interactions contextuelles, peur dynamique + battements de cœur, audio rodio (ambiance, boucles, SFX à gain distance), menus bilingues (héberger/rejoindre/options langue/sensibilité/nom), lobby, HUD (objectifs, tickets rotatifs, batteries, journal d'événements), écrans de fin.
- Corrections majeures pendant l'itération : propagation du sol aux cellules portes (bug de connectivité), réécriture de la progression des reboots (hold) et de check_exit dans le tick, bots (BFS au lieu d'un gel de chemin), API wgpu 22.1 (entry_point &str, desired_maximum_frame_latency), gltf features utils+names, collisions de noms lang/protocol, borrow checker de frame()/handle_msg/ai.
- Tests : 3 tests unitaires shared (carte, connectivité, roundtrip protocole), 3 tests unitaires server (reboot, note de frais → sortie → évasion → fin, audition entité), 1 test d'intégration TCP (binaire réel : Hello → CreateRoom → mauvais mot de passe rejeté → JoinRoom → StartGame → GameStarted → snapshots → Input répliqué → LeaveRoom). Flakiness corrigée (FrameReader persistant + port éphémère) → 5/5 verts.
- Builds : workspace complet (client audio inclus via ALSA local extrait de .deb) et release, 0 warning. Smoke test release : serveur + 2 bots, IA capture un bot, cycles états OK.
- Packaging : zip 8,7 Mo (sources + assets + tools + binaires release Linux) → /home/z/my-project/download/support-level-3.zip ; README bilingue (build, hébergement en ligne, commandes, architecture, régénération d'assets).

Stage Summary:
- Livrable : /home/z/my-project/download/support-level-3.zip (projet complet + binaires) ; sources dans /home/z/my-project/support-level-3/.
- Vérifié : cargo build (debug+release) 0 warning ; cargo test 7/7 (5/5 sur l'intégration réseau) ; smoke tests serveur avec bots (réparation des serveurs, captures de l'Auditeur, fin de partie) validés en debug et release.
- Limites assumées : pas de rendu headless dans le conteneur (pas de GPU/Vulkan) → la partie graphique est validée par compilation + structure, à tester visuellement sur machine de bureau ; audio Linux exige libasound2-dev (défaut), --no-default-features sinon.
