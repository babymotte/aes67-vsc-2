#!/bin/bash

sudo setcap cap_net_bind_service,cap_net_raw,cap_net_admin,cap_sys_time+ep /usr/sbin/ptp4l
