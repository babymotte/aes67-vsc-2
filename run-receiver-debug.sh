#!/bin/bash
. .env
cargo build || exit $?
sudo setcap 'cap_net_bind_service+eip cap_sys_nice+eip' ./target/debug/aes-vsc-receiver || exit $?
AES67_VSC_2_CONFIG="./config/receiver.yaml" ./target/debug/aes-vsc-receiver
# AES67_VSC_2_CONFIG="./config/receiver2.yaml" ./target/debug/aes-vsc-receiver
