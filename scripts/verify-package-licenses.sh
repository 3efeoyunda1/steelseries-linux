#!/bin/sh
set -eu

if [ "$#" -ne 3 ]; then
    echo "Usage: $0 <arch|debian|fedora> <package-name> <archive>" >&2
    exit 2
fi

repo_root=$(CDPATH= cd -P "$(dirname "$0")/.." && pwd)
format=$1
name=$2
archive=$3

fail() {
    echo "$archive: $*" >&2
    exit 1
}

case "$name" in
    steelseries-linux|steelseries-linux-gui) ;;
    *) fail "unexpected package name: $name" ;;
esac

case "$format" in
    arch|fedora) directory="usr/share/licenses/$name" ;;
    debian) directory="usr/share/doc/$name" ;;
    *) fail "unexpected package format: $format" ;;
esac

scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
trap 'exit 1' HUP INT TERM
case "$format" in
    arch) bsdtar -tf "$archive" > "$scratch/list" ;;
    debian)
        dpkg-deb --fsys-tarfile "$archive" > "$scratch/data.tar"
        tar -tf "$scratch/data.tar" > "$scratch/list"
        ;;
    fedora)
        rpm -K --nosignature "$archive"
        rpm -qp --qf '[%{FILENAMES}\n]' "$archive" > "$scratch/list"
        rpm -qp --qf '[%{FILENAMES}\t%{FILEDIGESTS}\t%{FILEFLAGS:fflags}\n]' \
            "$archive" > "$scratch/rpm-files"
        [ "$(rpm -qp --qf '%{FILEDIGESTALGO}' "$archive")" = 8 ] ||
            fail "expected SHA-256 RPM file digests"
        ;;
esac

if grep -Eq '(^|/)(about\.toml|about\.hbs|about\.md\.hbs)$' "$scratch/list"; then
    fail "contains a license generation template"
fi

verify_file() {
    source=$1
    path=$2
    [ -s "$source" ] || fail "missing or empty source file: $source"
    count=$(awk -v path="$path" '{ sub(/^\.?\//, ""); if ($0 == path) ++count } END { print count + 0 }' "$scratch/list")
    [ "$count" -eq 1 ] || fail "expected one /$path entry, found $count"

    case "$format" in
        arch)
            bsdtar -xOf "$archive" "$path" > "$scratch/content"
            cmp -s "$source" "$scratch/content" || fail "content mismatch for /$path"
            ;;
        debian)
            tar -xOf "$scratch/data.tar" "./$path" > "$scratch/content"
            cmp -s "$source" "$scratch/content" || fail "content mismatch for /$path"
            ;;
        fedora)
            digest=$(sha256sum < "$source")
            awk -F '\t' -v path="/$path" -v digest="${digest%% *}" \
                '$1 == path && $2 == digest && $3 ~ /l/ { found = 1 } END { exit !found }' \
                "$scratch/rpm-files" || fail "content mismatch or missing %license flag for /$path"
            ;;
    esac
}

verify_file "$repo_root/LICENSE" "$directory/LICENSE"
verify_file "$repo_root/THIRD_PARTY_LICENSES.md" "$directory/THIRD_PARTY_LICENSES.md"
if [ "$format" = debian ]; then
    sh "$repo_root/debian/render-copyright.sh" "$repo_root/debian/copyright" \
        "$repo_root/THIRD_PARTY_LICENSES.md" > "$scratch/copyright"
    verify_file "$scratch/copyright" "$directory/copyright"
fi
