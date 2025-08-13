#!/bin/bash
. .env
cargo build --release|| exit $?
sudo setcap 'cap_net_bind_service+eip cap_sys_nice+eip' ./target/release/aes-vsc-receiver || exit $?
AES67_VSC_2_CONFIG="./config/receiver.yaml" ./target/release/aes-vsc-receiver
# AES67_VSC_2_CONFIG="./config/receiver2.yaml" ./target/release/aes-vsc-receiver
