#!/bin/bash

systemctl --user disable --now aes67-jack-vsc.service || exit $?
exec ./install.sh