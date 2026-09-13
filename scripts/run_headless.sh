#!/bin/bash
# Lance le client SL3 sous Xvfb + lavapipe, avec autopilot.
# Usage : run_headless.sh "<SL3_AUTOPILOT>" [binaire] [logfile]
set -u
export PATH="$HOME/.local-mesa/xauthdir/usr/bin:$PATH"
export VK_ICD_FILENAMES="$HOME/.local-mesa/usr/share/vulkan/icd.d/lvp_icd.json"
export LD_LIBRARY_PATH="$HOME/.local-mesa/usr/lib/x86_64-linux-gnu"
export MESA_GL_VERSION_OVERRIDE=4.5
SCRIPT="${1:?scenario autopilot manquant}"
BIN="${2:-./target/release/sl3-client}"
LOG="${3:-/tmp/sl3_headless.log}"
# Serveur X dédié (numéro auto) + client dans le même processus de commande
XVFB_ARGS="-screen 0 1280x720x24 -nolisten tcp"
for N in 90 91 92 93 94 95; do
  if [ ! -e "/tmp/.X11-unix/X$N" ]; then DNUM=$N; break; fi
done
DNUM=${DNUM:-90}
rm -f "/tmp/.X11-unix/X$DNUM" "/tmp/.X$DNUM-lock"
setsid Xvfb ":$DNUM" $XVFB_ARGS > /tmp/sl3_xvfb.log 2>&1 &
XPID=$!
for i in $(seq 1 40); do
  [ -S "/tmp/.X11-unix/X$DNUM" ] && break
  sleep 0.1
done
export DISPLAY=":$DNUM"
"$BIN" > "$LOG" 2>&1 &
CPID=$!
wait $CPID
CODE=$?
kill $XPID 2>/dev/null
wait $XPID 2>/dev/null
exit $CODE
