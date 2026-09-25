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
rpm_topdir=$(mktemp -d)
trap 'rm -rf "$rpm_topdir"' EXIT HUP INT TERM

mkdir -p \
    "${rpm_topdir}/BUILD" \
    "${rpm_topdir}/BUILDROOT" \
    "${rpm_topdir}/RPMS" \
    "${rpm_topdir}/SOURCES" \
    "${rpm_topdir}/SPECS" \
    "${rpm_topdir}/SRPMS"

tar -czf "${rpm_topdir}/SOURCES/steelseries-linux-${version}.tar.gz" \
    --transform "s,^,steelseries-linux-${version}/," \
    -C "$repo_root" \
    Cargo.toml Cargo.lock crates udev LICENSE README.md

rpmbuild --define "_topdir ${rpm_topdir}" \
    -ba "${repo_root}/fedora/steelseries-linux.spec"

mkdir -p "${repo_root}/dist"
find "${rpm_topdir}/RPMS" -type f \
    -name "steelseries-linux-${version}-*.x86_64.rpm" \
    -exec cp -f {} "${repo_root}/dist/" \;
