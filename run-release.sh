#!/bin/bash

cargo build --release && sudo setcap 'cap_net_bind_service+eip cap_sys_nice+eip' ./target/release/aes-vsc-receiver || exit $?

. .env && AES67_VSC_2_CONFIG="./config/receiver.yaml" ./target/release/aes-vsc-receiver
