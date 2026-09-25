#!/bin/sh
set -eu

if [ "$(id -u)" -ne 0 ]; then
    exec sudo -- "$0" "$@"
fi

destination_rule="/usr/lib/udev/rules.d/70-steelseries-linux.rules"

rm -f -- "$destination_rule"
udevadm control --reload-rules
udevadm trigger --subsystem-match=hidraw

printf '%s\n' \
    "Removed SteelSeries Linux udev rules from ${destination_rule}." \
    "Already connected devices may need to be unplugged and reconnected for permissions to update."
