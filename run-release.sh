#!/bin/bash
. .env
cargo build --release || exit $?
sudo setcap 'cap_net_bind_service+eip cap_sys_nice+eip' ./target/release/aes-vsc-receiver || exit $?
./target/release/aes-vsc-receiver
