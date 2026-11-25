#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -P "$(dirname "$0")/.." && pwd)
. "${repo_root}/scripts/package-version.sh"
build_root=$(mktemp -d)
source_dir="${build_root}/steelseries-linux-${version}"
trap 'rm -rf -- "$build_root"' EXIT HUP INT TERM

mkdir -p "$source_dir"
cp -a \
    "${repo_root}/Cargo.toml" \
    "${repo_root}/Cargo.lock" \
    "${repo_root}/crates" \
    "${repo_root}/assets" \
    "${repo_root}/udev" \
    "${repo_root}/LICENSE" \
    "${repo_root}/README.md" \
    "${repo_root}/debian" \
    "$source_dir/"

# dpkg takes package metadata from the first changelog entry.
changelog="${source_dir}/debian/changelog"
current_version=$(dpkg-parsechangelog -l"$changelog" -S Version)
if [ "$current_version" != "${version}-1" ]; then
    {
        printf 'steelseries-linux (%s-1) unstable; urgency=medium\n\n' "$version"
        printf '  * Build from Cargo workspace version %s.\n\n' "$version"
        printf ' -- SteelSeries Linux <3efeoyunda1@users.noreply.github.com>  %s\n\n' "$(LC_ALL=C date -R)"
        cat "$changelog"
    } > "${changelog}.versioned"
    mv "${changelog}.versioned" "$changelog"
fi

tar -czf "${build_root}/steelseries-linux_${version}.orig.tar.gz" \
    --transform "s,^,steelseries-linux-${version}/," \
    -C "$repo_root" \
    Cargo.toml Cargo.lock crates assets udev LICENSE README.md

(
    cd "$source_dir"
    if [ "${STEELSERIES_EXTERNAL_RUST_TOOLCHAIN:-0}" = "1" ]; then
        external_rust_bin=${STEELSERIES_EXTERNAL_RUST_BIN:-}
        if [ -z "$external_rust_bin" ]; then
            if command -v cargo >/dev/null 2>&1; then
                external_rust_bin=$(dirname "$(command -v cargo)")
            elif [ -x /usr/local/cargo/bin/cargo ]; then
                # The official Rust Docker image installs here, but a login
                # shell may sanitize PATH before dpkg-buildpackage starts.
                external_rust_bin=/usr/local/cargo/bin
            else
                echo "external Rust toolchain requested, but cargo was not found" >&2
                exit 1
            fi
        fi
        external_cargo="${external_rust_bin}/cargo"
        external_rustc="${external_rust_bin}/rustc"
        external_rustdoc="${external_rust_bin}/rustdoc"
        for tool in "$external_cargo" "$external_rustc" "$external_rustdoc"; do
            if [ ! -x "$tool" ]; then
                echo "external Rust tool is not executable: $tool" >&2
                exit 1
            fi
        done
        CARGO="$external_cargo" \
            RUSTC="$external_rustc" \
            RUSTDOC="$external_rustdoc" \
            dpkg-buildpackage -b -us -uc -d
    else
        dpkg-buildpackage -b -us -uc
    fi
)

output_dir="${repo_root}/dist/debian"
mkdir -p "$output_dir"
for package in \
    "${build_root}"/steelseries-linux_"${version}"-1_*.deb \
    "${build_root}"/steelseries-linux-gui_"${version}"-1_*.deb
do
    [ -f "$package" ] || { echo "Missing Debian package for $version" >&2; exit 1; }
    name=$(dpkg-deb -f "$package" Package)
    files=$(dpkg-deb -c "$package")
    "${repo_root}/scripts/verify-package-completions.sh" "$name" <<EOF
$files
EOF
    cp -f "$package" "$output_dir/"
done
