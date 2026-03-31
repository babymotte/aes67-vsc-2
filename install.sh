#!/bin/bash

BIN="./target/release/aes67-rs-jack-vsc"
WEB_APP="./aes67-rs-vsc-web-ui/dist"
CONFIG_DIR="$HOME/.config/aes67-jack-vsc"
DATA_DIR="$HOME/.local/share/aes67-jack-vsc"
BIN_DIR="$HOME/.local/bin"
SYSTEMD_USER_DIR="$HOME/.config/systemd/user"
SYSTEMD_DIR="/etc/systemd/system"

# build the application
cargo build --package aes67-rs-jack-vsc --release || exit $?
cd aes67-rs-vsc-web-ui && npm install && npm run build && cd .. || exit $?

# create directories
mkdir -p "$CONFIG_DIR/routing" || exit $?
mkdir -p "$DATA_DIR/data" || exit $?
mkdir -p "$DATA_DIR/html" || exit $?
mkdir -p "$BIN_DIR" || exit $?
mkdir -p "$SYSTEMD_USER_DIR" || exit $?

# clear old files
rm -rf "$DATA_DIR/html/*"

systemctl --user stop aes67-jack-vsc.service

# copy files
cp -r "$WEB_APP"/* "$DATA_DIR/html/" || exit $?
cp "$BIN" "$BIN_DIR" || exit $?
cp ./config.yaml "$CONFIG_DIR" || exit $?
cp ./systemd/aes67-jack-vsc.service "$SYSTEMD_USER_DIR" || exit $?
sudo cp ./systemd/ptp4l@.service "$SYSTEMD_DIR" || exit $?
sudo mkdir -p /etc/linuxptp || exit $?
sudo cp ./linuxptp/ptp4l.conf /etc/linuxptp/ptp4l.conf || exit $?

# set permissions
sudo setcap 'cap_net_bind_service+ep cap_sys_nice+ep cap_sys_time+ep cap_net_admin+ep' "$BIN_DIR/aes67-rs-jack-vsc" || exit $?

# create ptp and audio groups
sudo groupadd -f ptp || exit $?

# add udev rules
sudo cp ./udev/99-ptp.rules /etc/udev/rules.d/99-ptp.rules || exit $?

# reload udev rules
sudo udevadm control --reload-rules && sudo udevadm trigger || exit $?

sudo usermod -aG ptp $USER || exit $?

# enable service
systemctl --user daemon-reload || exit $?
systemctl --user enable --now aes67-jack-vsc.service || exit $?

# enable ptp4l service
for iface in /sys/class/net/*/; do
    name=$(basename "$iface")
    caps=$(ethtool -T "$name" 2>/dev/null)
    if echo "$caps" | grep -q "hardware-transmit\|hardware-receive\|hardware-raw-clock"; then
        sudo systemctl enable --now "ptp4l@$name.service"
        echo "Enabled ptp4l@$name.service for interface $name"
    fi
done


echo "Open http://localhost:43567 in your browser"