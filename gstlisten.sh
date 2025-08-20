#!/bin/bash

gst-launch-1.0 udpsrc address=239.69.232.56 port=5004 ! \
    application/x-rtp, clock-rate=48000, channels=2 ! \
    rtpjitterbuffer ! rtpL24depay ! audioconvert ! audioresample ! autoaudiosink