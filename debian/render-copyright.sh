#!/bin/sh
set -eu

if [ "$#" -ne 2 ] || [ ! -s "$1" ] || [ ! -s "$2" ]; then
    echo "Usage: $0 <debian/copyright> <THIRD_PARTY_LICENSES.md> (both required)" >&2
    exit 1
fi

# Include the notices in the DEP-5 header's Comment field. Continuation
# lines preserve the Markdown text, including its paragraph breaks.
awk '
    NR == FNR {
        report[++lines] = $0
        next
    }
    !inserted && /^$/ {
        print " ."
        for (i = 1; i <= lines; ++i)
            print (report[i] == "" ? " ." : " " report[i])
        inserted = 1
    }
    { print }
    END { if (!inserted) exit 1 }
' "$2" "$1"
