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
    "${repo_root}/assets" \
    "${repo_root}/udev" \
    "${repo_root}/LICENSE" \
    "${repo_root}/README.md" \
    "${repo_root}/debian" \
    "$source_dir/"

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
    [ -f "$package" ] || continue
    cp -f "$package" "$output_dir/"
done
