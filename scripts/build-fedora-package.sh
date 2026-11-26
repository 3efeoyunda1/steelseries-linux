#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -P "$(dirname "$0")/.." && pwd)
. "${repo_root}/scripts/package-version.sh"
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
    Cargo.toml Cargo.lock crates assets udev LICENSE THIRD_PARTY_LICENSES.md README.md

# rpmbuild reads Version from the spec before unpacking sources.
sed "s/^Version:[[:space:]]*.*/Version:        ${version}/" \
    "${repo_root}/fedora/steelseries-linux.spec" > "${rpm_topdir}/SPECS/steelseries-linux.spec"
rpmbuild --define "_topdir ${rpm_topdir}" \
    -ba "${rpm_topdir}/SPECS/steelseries-linux.spec"

output_dir="${repo_root}/dist/fedora"
mkdir -p "$output_dir"
for name in steelseries-linux steelseries-linux-gui; do
    found=false
    for package in "${rpm_topdir}"/RPMS/*/"${name}-${version}-"*.rpm; do
        [ -f "$package" ] || continue
        files=$(rpm -qpl "$package")
        "${repo_root}/scripts/verify-package-completions.sh" "$name" <<EOF
$files
EOF
        "${repo_root}/scripts/verify-package-licenses.sh" fedora "$name" "$package"
        cp -f "$package" "$output_dir/"
        found=true
    done
    [ "$found" = true ] || { echo "Missing Fedora package $name for $version" >&2; exit 1; }
done
