#!/bin/sh
set -eu
IFS= read -r request
if [ "$request" != STOP ]; then
    printf 'INVALID\n'
    exit 1
fi
pid=$(cat /run/vmux-init.pid)
printf 'STOPPING\n'
kill -TERM "$pid"
