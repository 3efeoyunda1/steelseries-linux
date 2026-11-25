#!/bin/sh
set -eu

case "${1:-}" in
    steelseries-linux) expected=1 ;;
    steelseries-linux-gui) expected=0 ;;
    *) echo "Usage: $0 <steelseries-linux|steelseries-linux-gui> < package-file-list" >&2; exit 2 ;;
esac

awk -v expected="$expected" '
    BEGIN {
        paths["usr/share/bash-completion/completions/steelseriesctl"] = 0
        paths["usr/share/zsh/site-functions/_steelseriesctl"] = 0
        paths["usr/share/fish/vendor_completions.d/steelseriesctl.fish"] = 0
    }
    {
        path = $NF
        sub(/^\.?\//, "", path)
        if (path in paths)
            paths[path]++
    }
    END {
        for (path in paths)
            if (paths[path] != expected) {
                printf "Expected %d entries for /%s, found %d\n", expected, path, paths[path] > "/dev/stderr"
                failed = 1
            }
        exit failed
    }
'
