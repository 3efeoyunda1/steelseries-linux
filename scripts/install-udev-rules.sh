#!/bin/sh
set -eu

if [ "$(id -u)" -ne 0 ]; then
    exec sudo -- "$0" "$@"
fi

script_dir=$(CDPATH= cd -P "$(dirname "$0")" && pwd)
source_rule="${script_dir}/../udev/70-steelseries-linux.rules"
destination_rule="/usr/lib/udev/rules.d/70-steelseries-linux.rules"

install -D -m 0644 "$source_rule" "$destination_rule"
udevadm control --reload-rules
udevadm trigger --subsystem-match=hidraw

printf '%s\n' \
    "Installed SteelSeries Linux udev rules to ${destination_rule}." \
    "Already connected devices may need to be unplugged and reconnected before the new ACL applies."
