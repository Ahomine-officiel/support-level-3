#!/bin/bash
# Build SL3 avec les libs locales (ALSA dev extrait des .deb, x11).
# Usage : bash ~/.local/build_sl3.sh [args cargo supplémentaires]
set -eu
export PATH="$HOME/.cargo/bin:$PATH"
export PKG_CONFIG_PATH="$HOME/.local-alsa/usr/lib/x86_64-linux-gnu/pkgconfig"
export RUSTFLAGS="-L native=$HOME/.local-alsa/usr/lib/x86_64-linux-gnu -L native=$HOME/.local-x11/usr/lib/x86_64-linux-gnu"
cd /home/z/my-project/support-level-3
cargo build --release "$@"
