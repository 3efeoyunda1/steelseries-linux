#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -P "$(dirname "$0")/.." && pwd)
version=$(awk '
    /^\[workspace.package\]$/ { in_workspace_package = 1; next }
    /^\[/ { in_workspace_package = 0 }
    in_workspace_package && /^version = / {
        gsub(/["[:space:]]/, "", $3)
        print $3
        exit
    }
' "${repo_root}/Cargo.toml")
build_root=$(mktemp -d)
source_dir="${build_root}/steelseries-linux-${version}"
trap 'rm -rf -- "$build_root"' EXIT HUP INT TERM

mkdir -p "$source_dir"
cp -a \
    "${repo_root}/Cargo.toml" \
    "${repo_root}/Cargo.lock" \
    "${repo_root}/crates" \
    "${repo_root}/udev" \
    "${repo_root}/LICENSE" \
    "${repo_root}/README.md" \
    "${repo_root}/debian" \
    "$source_dir/"

tar -czf "${build_root}/steelseries-linux_${version}.orig.tar.gz" \
    --transform "s,^,steelseries-linux-${version}/," \
    -C "$repo_root" \
    Cargo.toml Cargo.lock crates udev LICENSE README.md

(
    cd "$source_dir"
    if [ "${STEELSERIES_EXTERNAL_RUST_TOOLCHAIN:-0}" = "1" ]; then
        dpkg-buildpackage -b -us -uc -d
    else
        dpkg-buildpackage -b -us -uc
    fi
)

mkdir -p "${repo_root}/dist"
for package in "${build_root}"/steelseries-linux_"${version}"-1_*.deb; do
    [ -f "$package" ] || continue
    cp -f "$package" "${repo_root}/dist/"
done
