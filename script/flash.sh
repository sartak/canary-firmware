#!/bin/bash

echo "Put device in BOOTSEL mode..."
while ! picotool info -d 2>/dev/null | grep -q "type: *RP2040"; do
    sleep 0.5
    echo -n "."
done
echo ""
echo "Device found, flashing..."
picotool load --update --verify --execute -t elf "$@"

if [[ "$1" == *"/debug/"* ]]; then
    echo "Debug build detected, waiting for USB serial device..."
    sleep 2  # Give the device time to connect
    ./script/log.sh
fi
