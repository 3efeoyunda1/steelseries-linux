#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -P "$(dirname "$0")/.." && pwd)
package_dir="${repo_root}/arch"
. "${repo_root}/scripts/package-version.sh"
build_root=$(mktemp -d)
trap 'rm -rf -- "$build_root"' EXIT HUP INT TERM
source_archive="${build_root}/steelseries-linux-${version}.tar.gz"

cp \
    "${package_dir}/PKGBUILD" \
    "${package_dir}/steelseries-linux.install" \
    "$build_root/"

# makepkg reads pkgver before extracting the source archive.
sed "s/^pkgver=.*/pkgver=${version}/" "${build_root}/PKGBUILD" > "${build_root}/PKGBUILD.versioned"
mv "${build_root}/PKGBUILD.versioned" "${build_root}/PKGBUILD"

tar -czf "$source_archive" \
    --transform "s,^,steelseries-linux-${version}/," \
    -C "$repo_root" \
    Cargo.toml Cargo.lock crates assets udev LICENSE README.md

(
    cd "$build_root"
    makepkg -Csf
)

output_dir="${repo_root}/dist/arch"
mkdir -p "$output_dir"
for package in "${build_root}"/steelseries-linux*-"${version}"-*.pkg.tar.zst; do
    [ -f "$package" ] || { echo "No Arch packages produced for $version" >&2; exit 1; }
    name=$(pacman -Qqp "$package")
    files=$(pacman -Qlp "$package")
    "${repo_root}/scripts/verify-package-completions.sh" "$name" <<EOF
$files
EOF
    cp -f "$package" "$output_dir/"
done
