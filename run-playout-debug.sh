#!/bin/bash
. .env
cargo build || exit $?
sudo setcap 'cap_net_bind_service+eip cap_sys_nice+eip' ./target/debug/aes-vsc-jack-playout || exit $?
AES67_VSC_2_CONFIG="./config/playout.yaml" ./target/debug/aes-vsc-jack-playout
