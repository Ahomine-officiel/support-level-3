#!/bin/bash
# Preuve A/B du coût des rayons : UNE seule partie, MÊME machine, MÊME résolution,
# on bascule le mode RT en jeu (touche T) et on mesure le FPS à chaque palier.
# off -> qualité (T) -> ultra (T) -> overdrive (T). 4 captures, 4 mesures [bench].
set -u
export PATH="$HOME/.cargo/bin:$PATH"
cd /home/z/my-project/support-level-3

export SL3_WINDOW_SIZE=1280x720
export SL3_BLACKOUT_MIN=10000
export SL3_BLACKOUT_MAX=10020
export SL3_AUDITOR=0
export VK_ICD_FILENAMES="$HOME/.local-mesa/usr/share/vulkan/icd.d/lvp_icd.json"
export LD_LIBRARY_PATH="$HOME/.local-mesa/usr/lib/x86_64-linux-gnu:$HOME/.local-x11/usr/lib/x86_64-linux-gnu:$HOME/.local-alsa/usr/lib/x86_64-linux-gnu"
export MESA_GL_VERSION_OVERRIDE=4.5
export SL3_DEBUG=1

CAP=/home/z/my-project/download/captures
mkdir -p "$CAP"

# Config fraîche : RT OFF au départ, rendu natif 1.0, pas d'upscaler.
cat > target/release/sl3_config.json <<'EOF'
{"name":"Tech-07","lang":"fr","host_default":"127.0.0.1:27070","sensitivity":1.0,"volume":0.8,"render_scale":1.0,"rt_mode":0,"upscaler":0,"upscale_quality":1}
EOF

# Chronologie (le jeu démarre ~t=19 sous lavapipe) :
#   t=28  shot OFF          (8+ frames en jeu, fenêtre bench fraîche)
#   t=30  T -> Qualité      (7 r/px)
#   t=46  shot Qualité      (16 s écoulées ≈ 20 frames, fenêtre 100 % post-bascule)
#   t=48  T -> Ultra        (14 r/px)
#   t=66  shot Ultra        (18 s ≈ 16 frames @1.1 s)
#   t=68  T -> Overdrive    (26 r/px, path tracing)
#   t=92  shot Overdrive    (24 s ≈ 15 frames @1.6 s)
#   t=96  exit
SCENARIO="click 640 310@6; click 482 392@8; key Enter@11; click 640 554@18; tp 41 43@24; yaw 90@26; \
shot $CAP/rt_ab_0_off.png@28; key F5@30; \
shot $CAP/rt_ab_1_qualite.png@46; key F5@48; \
shot $CAP/rt_ab_2_ultra.png@66; key F5@68; \
shot $CAP/rt_ab_3_overdrive.png@92; exit@96"

bash /home/z/my-project/scripts/run_headless.sh "$SCENARIO" ./target/release/sl3-client /tmp/sl3_rt_ab.log
echo "=== run A/B terminé ; mesures :"
grep -E "\[bench\]" /tmp/sl3_rt_ab.log
