#!/bin/bash
# Bench + captures RT 4 modes sous lavapipe : off / qualité / ultra / overdrive.
# Pour chaque mode : config fraîche -> client autopilot (Héberger intégré ->
# salon -> Démarrer -> tp allée néons -> capture). Le tag perf à l'écran porte
# le FPS mesuré + le compteur de rayons/px : preuve du coût réel.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
cd /home/z/my-project/support-level-3

export SL3_WINDOW_SIZE=1280x720
export SL3_BLACKOUT_MIN=10000
export SL3_BLACKOUT_MAX=10020
export VK_ICD_FILENAMES="$HOME/.local-mesa/usr/share/vulkan/icd.d/lvp_icd.json"
export LD_LIBRARY_PATH="$HOME/.local-mesa/usr/lib/x86_64-linux-gnu:$HOME/.local-x11/usr/lib/x86_64-linux-gnu:$HOME/.local-alsa/usr/lib/x86_64-linux-gnu"
export MESA_GL_VERSION_OVERRIDE=4.5

CAP=/home/z/my-project/download/captures
mkdir -p "$CAP"

MODE=${1:?mode 0|1|2|3}
TAG=${2:?tag nom capture}
SHOT="$CAP/rt_pt_${TAG}.png"

# Config fraîche avant CHAQUE run (le client sauvegarde à la sortie -> sinon décalage).
cat > target/release/sl3_config.json <<EOF
{"name":"Tech-07","lang":"fr","host_default":"127.0.0.1:27070","sensitivity":1.0,"volume":0.8,"render_scale":1.0,"rt_mode":$MODE,"upscaler":0,"upscale_quality":1}
EOF

# Chronologie généreuse : lavapipe rend 0.5-6 s/frame en jeu (overdrive le plus lent).
# yaw 90 = le long du couloir (forward = (sin yaw, _, cos yaw) -> +x).
SCENARIO="click 640 310@6; key Enter@11; click 640 554@18; tp 41 43@30; yaw 90@32; look 0 82@34; shot $SHOT@46; exit@49"
export SL3_DEBUG=1

bash /home/z/my-project/scripts/run_headless.sh "$SCENARIO" ./target/release/sl3-client /tmp/sl3_rt_${TAG}.log
echo "=== mode $MODE ($TAG) terminé : $SHOT"
ls -la "$SHOT" 2>/dev/null || tail -5 /tmp/sl3_rt_${TAG}.log
