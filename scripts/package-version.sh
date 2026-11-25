#!/bin/sh
# Source this file after setting repo_root to the repository root.
version=$(awk '
    /^\[workspace.package\]$/ { in_workspace_package = 1; next }
    /^\[/ { in_workspace_package = 0 }
    in_workspace_package && /^version[[:space:]]*=/ {
        gsub(/["[:space:]]/, "", $3)
        print $3
        exit
    }
' "${repo_root}/Cargo.toml")
if ! printf '%s\n' "$version" | LC_ALL=C grep -Eq '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'; then
    printf 'Invalid Cargo workspace version: %s\n' "$version" >&2
    exit 1
fi
