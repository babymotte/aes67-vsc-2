#!/bin/bash

echo 'KERNEL=="ptp[0-9]*", GROUP="audio", MODE="0660"' | sudo tee /etc/udev/rules.d/99-phc.rules
sudo udevadm control --reload
sudo udevadm trigger