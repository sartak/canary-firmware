#!/bin/bash

TIMEOUT=5
INTERVAL=0.1

echo "Waiting for USB serial device..."
end_time=$((SECONDS + TIMEOUT))

while [ $SECONDS -lt $end_time ]; do
  TTY_DEVICE=$(ls /dev/tty.usbmodem* 2>/dev/null | head -n 1)
  if [ -n "$TTY_DEVICE" ]; then
    tio -b 115200 --timestamp "$TTY_DEVICE"
    exit 0
  fi
  sleep "$INTERVAL"
done

echo "No USB serial device found within $TIMEOUT seconds"
exit 1
