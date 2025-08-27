#!/bin/bash

cargo build || exit 1
cd ./test/alsa-receiver
make BUILD=debug && make debug