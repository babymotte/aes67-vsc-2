#!/bin/bash

BIN="./target/release/aes67-rs-jack-vsc"
WEB_APP="./aes67-rs-vsc-web-ui/dist"
CONFIG_DIR="$HOME/.config/aes67-jack-vsc"
DATA_DIR="$HOME/.local/share/aes67-jack-vsc"
BIN_DIR="$HOME/.local/bin"
SYSTEMD_DIR="$HOME/.config/systemd/user"

# build the application
cargo build --package aes67-rs-jack-vsc --release || exit $?
cd aes67-rs-vsc-web-ui && npm install && npm run build && cd .. || exit $?

# create directories
mkdir -p "$CONFIG_DIR/routing" || exit $?
mkdir -p "$DATA_DIR/data" || exit $?
mkdir -p "$DATA_DIR/html" || exit $?
mkdir -p "$BIN_DIR" || exit $?
mkdir -p "$SYSTEMD_DIR" || exit $?

# clear old files
rm -rf "$DATA_DIR/html/*"

# copy files
cp -r "$WEB_APP"/* "$DATA_DIR/html/" || exit $?
cp "$BIN" "$BIN_DIR" || exit $?
cp ./config.yaml "$CONFIG_DIR" || exit $?
cp ./systemd/aes67-jack-vsc.service "$SYSTEMD_DIR" || exit $?

# set permissions
sudo setcap 'cap_net_bind_service+eip cap_sys_nice+eip' "$BIN_DIR/aes67-rs-jack-vsc" || exit $?

# enable service
systemctl --user daemon-reload || exit $?
systemctl --user enable --now aes67-jack-vsc.service || exit $?

echo "Open http://localhost:43567 in your browser"