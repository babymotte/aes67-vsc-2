#!/bin/bash
. .env
pw-metadata -n settings 0 clock.force-quantum 96
RUSTFLAGS="--cfg tokio_unstable" cargo build --package aes67-rs-jack-vsc --features=tokio-console,tokio-metrics || exit $?
# sudo setcap 'cap_net_bind_service+eip cap_sys_nice+eip' ./target/debug/aes67-rs-jack-vsc || exit $?
AES67_VSC_2_CONFIG="./aes67-rs-jack-vsc/config/vsc.yaml" ./target/debug/aes67-rs-jack-vsc
# AES67_VSC_2_CONFIG="./config/receiver2.yaml" ./target/debug/aes-vsc-receiver
