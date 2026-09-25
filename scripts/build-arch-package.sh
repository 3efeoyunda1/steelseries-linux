#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -P "$(dirname "$0")/.." && pwd)
package_dir="${repo_root}/arch"
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
trap 'rm -rf -- "$build_root"' EXIT HUP INT TERM
source_archive="${build_root}/steelseries-linux-${version}.tar.gz"

cp \
    "${package_dir}/PKGBUILD" \
    "${package_dir}/steelseries-linux.install" \
    "$build_root/"

tar -czf "$source_archive" \
    --transform "s,^,steelseries-linux-${version}/," \
    -C "$repo_root" \
    Cargo.toml Cargo.lock crates udev LICENSE README.md

(
    cd "$build_root"
    makepkg -Csf
)

mkdir -p "${repo_root}/dist"
for package in "${build_root}"/steelseries-linux-"${version}"-*.pkg.tar.zst; do
    [ -f "$package" ] || continue
    cp -f "$package" "${repo_root}/dist/"
done
