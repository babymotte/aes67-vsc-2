#!/bin/bash

systemctl --user stop aes67-jack-vsc.service
. .env
# RUSTFLAGS="--cfg tokio_unstable" cargo build --package aes67-rs-jack-vsc --features=tokio-console,tokio-metrics || exit $?

# pw-metadata -n settings 0 clock.force-quantum 1024

# for _ in {1..999}; do
    cargo build --package aes67-rs-jack-vsc || exit $?
    sudo setcap 'cap_net_bind_service+ep cap_sys_nice+ep cap_sys_time+ep cap_net_admin+ep' ./target/debug/aes67-rs-jack-vsc || exit $?
    RUST_BACKTRACE=1 RUST_LOG="aes67_rs_ui=info,aes67_rs_jack_vsc::session_manager=warn,aes67_rs::monitoring=warn,statime=warn,worterbuch=warn,tosub=warn,sap_rs=warn,info" pw-jack -p 192 ./target/debug/aes67-rs-jack-vsc --config ./config.yaml --data-dir ./data
# done

# AES67_VSC_2_CONFIG="./config/receiver2.yaml" ./target/debug/aes-vsc-receiver
