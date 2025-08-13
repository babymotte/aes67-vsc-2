#!/bin/bash
. .env
cargo build || exit $?
sudo setcap 'cap_net_bind_service+eip cap_sys_nice+eip' ./target/debug/aes-vsc-receiver || exit $?
./target/debug/aes-vsc-receiver
