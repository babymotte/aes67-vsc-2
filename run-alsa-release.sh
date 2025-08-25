#!/bin/bash

cargo build --release || exit 1
cd ./test/alsa-receiver
make && make cap || exit 1
sudo cp ../../target/release/libaes67_vsc_2.so /usr/lib/ && sudo chown root:root /usr/lib/libaes67_vsc_2.so || exit 1
make run