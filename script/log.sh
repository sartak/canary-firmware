#!/bin/bash
TTY_DEVICE=$(ls /dev/tty.usbmodem* 2>/dev/null | head -n 1)

if [ -n "$TTY_DEVICE" ]; then
  tio -b 115200 --timestamp "$TTY_DEVICE"
else
  echo "No USB serial device found."
fi
