#!/bin/bash
# Re-package support-level-3.zip : sources + assets + tools + binaires release Linux.
set -e
cd /home/z/my-project
rm -f download/support-level-3.zip
# Binaires release à jour dans bin/
mkdir -p support-level-3/bin
cp -f support-level-3/target/release/sl3-client support-level-3/bin/
cp -f support-level-3/target/release/sl3-server support-level-3/bin/
# Zip (sans target/, avec assets/, tools/, bin/, README)
cd support-level-3
zip -qr /home/z/my-project/download/support-level-3.zip \
  README.md Cargo.toml Cargo.lock crates assets tools bin \
  -x "crates/client/target/*" -x "target/*"
cd ..
ls -la download/support-level-3.zip
unzip -l download/support-level-3.zip | tail -2
