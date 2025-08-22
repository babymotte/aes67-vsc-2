#!/bin/bash

cargo build --bin aes-vsc-receiver --release && sudo setcap 'cap_net_bind_service+eip cap_sys_nice+eip' ./target/release/aes-vsc-receiver || exit $?

. .env && AES67_VSC_2_CONFIG="./config/receiver-local.yaml" ./target/release/aes-vsc-receiver
. .env && AES67_VSC_2_CONFIG="./config/receiver-local2.yaml" ./target/release/aes-vsc-receiver
