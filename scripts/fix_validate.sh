#!/bin/bash
# Validation des fixes joueur : début calme, souris non inversée (raw input),
# batterie visible, textures non cassées (RT off puis RT qualité).
# Usage : bash scripts/fix_validate.sh
set -u
cd /home/z/my-project/support-level-3
export PATH="$HOME/.cargo/bin:$PATH"
S=/home/z/my-project/scripts

export SL3_WINDOW_SIZE=1280x720
export SL3_BLACKOUT_MIN=10000
export SL3_BLACKOUT_MAX=10020
export SL3_AUDITOR=0
export SL3_DEBUG=1

CAP=/home/z/my-project/download/captures
mkdir -p "$CAP"

# Config fraîche : RT OFF, rendu natif, pas d'upscaler.
cat > target/release/sl3_config.json <<'EOF'
{"name":"Tech-07","lang":"fr","host_default":"127.0.0.1:27070","sensitivity":1.0,"volume":0.8,"render_scale":1.0,"rt_mode":0,"upscaler":0,"upscale_quality":1}
EOF

MODE="${1:-calm}"

if [ "$MODE" = "calm" ]; then
  # Début CALME + souris + batterie :
  #   t=26  shot début de partie (calme=1 : site éclairé, grain ~invisible)
  #   t=36  tp (21,10) : la batterie 'b' la plus proche est à (21,7) -> 3 m nord
  #   t=38  yaw +180 (face -Z, vers la batterie) ; t=40 regard baissé 24°
  #   t=46  shot batterie (glow cyan + lévitation/rotation visibles)
  #   t=48  look 500 px droite -> yaw doit DIMINUER (non inversé)
  #   t=56  shot après rotation (la scène doit avoir pivoté vers la droite)
  export SL3_AUTOPILOT="click 640 310@6; click 482 392@8; key Enter@11; click 640 554@18; \
shot $CAP/fix_calm_start.png@26; tp 21 10@36; yaw 180@38; look 0 200@40; \
shot $CAP/fix_battery.png@46; yaw 180@48; look 500 0@49; shot $CAP/fix_look_right.png@56; \
tp 38 43@58; yaw 90@60; shot $CAP/fix_tex_wall.png@68; exit@72"
  bash $S/run_headless.sh "$SL3_AUTOPILOT" ./target/release/sl3-client /tmp/sl3_fix_calm.log
  grep -E "sl3-debug|autopilot" /tmp/sl3_fix_calm.log | tail -8
else
  # Sol RT sans traînées : même départ, tp couloir néon, F5 = Qualité.
  export SL3_AUTOPILOT="click 640 310@6; click 482 392@8; key Enter@11; click 640 554@18; \
tp 41 43@24; yaw 90@26; key F5@28; shot $CAP/fix_rt_floor.png@46; exit@50"
  bash $S/run_headless.sh "$SL3_AUTOPILOT" ./target/release/sl3-client /tmp/sl3_fix_rt.log
  grep -E "sl3-debug|autopilot" /tmp/sl3_fix_rt.log | tail -6
fi
