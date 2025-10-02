#!/bin/bash
. .env
cargo build --package aes67-rs-jack-vsc --release || exit $?
# sudo setcap 'cap_net_bind_service+eip cap_sys_nice+eip' ./target/debug/aes67-rs-jack-vsc || exit $?
AES67_VSC_2_CONFIG="./aes67-rs-jack-vsc/config/vsc.yaml" ./target/release/aes67-rs-jack-vsc
# AES67_VSC_2_CONFIG="./config/receiver2.yaml" ./target/debug/aes-vsc-receiver
