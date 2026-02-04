#!/bin/bash

cargo build --package aes67-rs-jack-vsc --release || exit $?
cd aes67-rs-vsc-web-ui && npm install && npm run build && cd .. || exit $?
sudo mkdir -p /usr/lib/aes67-jack-vsc/static || exit 1
sudo mkdir -p /etc/aes67-jack-vsc || exit 1
sudo mkdir -p /var/lib/aes67-jack-vsc/data || exit 1
sudo cp -r aes67-rs-vsc-web-ui/dist/* /usr/lib/aes67-jack-vsc/static/ || exit $?
sudo cp ./target/release/aes67-rs-jack-vsc /usr/local/bin || exit 1
sudo cp ./config.yaml /etc/aes67-jack-vsc/config.yaml || exit 1
sudo setcap 'cap_net_bind_service+eip cap_sys_nice+eip' /usr/local/bin/aes67-rs-jack-vsc || exit $?
sudo cp ./systemd/aes67-jack-vsc.service /etc/systemd/system/ || exit 1
sudo systemctl daemon-reload || exit 1
sudo systemctl enable --now aes67-jack-vsc.service || exit 1
xdg-open http://localhost:43567