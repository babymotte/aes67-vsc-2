#!/bin/bash
. .env
pw-metadata -n settings 0 clock.force-quantum 96
cargo build --package aes67-rs-jack-vsc --release || exit $?
sudo setcap 'cap_net_bind_service+eip cap_sys_nice+eip' ./target/release/aes67-rs-jack-vsc || exit $?
RUST_LOG="worterbuch=error,aes67_rs_ui=warn,aes67_rs_jack_vsc=warn,aes67_rs::monitoring=warn,statime=warn,info"  ./target/release/aes67-rs-jack-vsc
# AES67_VSC_2_CONFIG="./config/receiver2.yaml" ./target/debug/aes-vsc-receiver
