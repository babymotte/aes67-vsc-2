#!/bin/bash

cargo build --release || exit 1
cd ./test/alsa-receiver
make clean && make && make run