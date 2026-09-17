#!/bin/bash
# Reconstruction de l'environnement de build SL3 (conteneur reset).
# rustup minimal + ALSA + x11 + lavapipe (ICD patché chemin absolu) + xauth.
set -eu
export DEBIAN_FRONTEND=noninteractive
D=/home/z/my-project
cd $D

echo "== 1. rustup minimal =="
if [ ! -x "$HOME/.cargo/bin/cargo" ]; then
  sh $D/scripts/rustup-init.sh -y --profile minimal --default-toolchain stable >/tmp/rustup.log 2>&1
fi
export PATH="$HOME/.cargo/bin:$PATH"
rustc --version; cargo --version

echo "== 2. ALSA dev -> ~/.local-alsa =="
mkdir -p ~/.local-alsa
dpkg-deb -x $D/libasound2-dev_*.deb ~/.local-alsa/
dpkg-deb -x $D/libasound2t64_*.deb ~/.local-alsa/
# libasound.so (dev) pointe vers libasound.so.2.0.0 (runtime) ; fallback -> lib systeme
[ -e ~/.local-alsa/usr/lib/x86_64-linux-gnu/libasound.so.2.0.0 ] || \
  ln -sf /usr/lib/x86_64-linux-gnu/libasound.so.2 ~/.local-alsa/usr/lib/x86_64-linux-gnu/libasound.so
# patch alsa.pc : prefix -> local (le .deb pointe /usr, inexistant dans le conteneur)
sed -i "s#^prefix=.*#prefix=$HOME/.local-alsa/usr#" ~/.local-alsa/usr/lib/x86_64-linux-gnu/pkgconfig/alsa.pc

echo "== 3. x11 -> ~/.local-x11 =="
mkdir -p ~/.local-x11
for deb in $D/libxcb-cursor0_*.deb $D/libxcb-xkb1_*.deb $D/libxkbcommon-x11-0_*.deb; do
  dpkg-deb -x "$deb" ~/.local-x11/
done

echo "== 4. lavapipe -> ~/.local-mesa =="
mkdir -p ~/.local-mesa
dpkg-deb -x "$D/mesa-vulkan-drivers_25.0.7-2+deb13u1_amd64.deb" ~/.local-mesa/
# ICD : library_path doit etre un chemin ABSOLU
ICD=~/.local-mesa/usr/share/vulkan/icd.d/lvp_icd.json
sed -i "s#\"library_path\"[^,]*#\"library_path\": \"$HOME/.local-mesa/usr/lib/x86_64-linux-gnu/libvulkan_lvp.so\"#" "$ICD"
grep -o '"library_path": "[^"]*"' "$ICD"

echo "== 5. xauth =="
mkdir -p ~/.local-mesa/xauthdir
dpkg-deb -x "$D/xauth_1%3a1.1.2-1.1_amd64.deb" ~/.local-mesa/xauthdir/ 2>/dev/null || true

echo "== 6. ALSA runtime (libasound.so.2 systeme pour le client) =="
mkdir -p ~/.local-alsa/usr/lib/x86_64-linux-gnu
[ -e /usr/lib/x86_64-linux-gnu/libasound.so.2 ] || echo "WARN: pas de libasound.so.2 systeme"
ls -la ~/.local-alsa/usr/lib/x86_64-linux-gnu/ | head -5

echo "ENV OK"
