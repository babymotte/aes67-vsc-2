#!/bin/bash

cargo build || exit 1
cd ./test/alsa-receiver
make clean && make BUILD=debug && make debug