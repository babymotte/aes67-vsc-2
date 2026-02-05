#!/bin/bash
. .env
pw-metadata -n settings 0 clock.force-quantum 96
# RUSTFLAGS="--cfg tokio_unstable" cargo build --package aes67-rs-jack-vsc --features=tokio-console,tokio-metrics || exit $?
cargo build --package aes67-rs-jack-vsc --release || exit $?
sudo setcap 'cap_net_bind_service+eip cap_sys_nice+eip' ./target/release/aes67-rs-jack-vsc || exit $?

for _ in {1..999}; do
    sleep 0.1 && RUST_BACKTRACE=1 RUST_LOG="aes67_rs_ui=warn,aes67_rs_jack_vsc=warn,aes67_rs::monitoring=warn,statime=warn,info" ./target/release/aes67-rs-jack-vsc --config ./config.yaml --data-dir ./data
done

# AES67_VSC_2_CONFIG="./config/receiver2.yaml" ./target/debug/aes-vsc-receiver
