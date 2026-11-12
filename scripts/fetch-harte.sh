#!/bin/sh
# Fetch Tom Harte's NMOS 6502 SingleStepTests (MIT licence) into a cache
# directory, default target/test-data/harte. About 1.3 GB; files already
# present are kept. Usage: sh scripts/fetch-harte.sh [dir]
set -eu
cd "$(dirname "$0")/.."
dir="${1:-target/test-data/harte}"
mkdir -p "$dir"
base=https://raw.githubusercontent.com/SingleStepTests/65x02/main/6502/v1
i=0
while [ "$i" -le 255 ]; do
    f=$(printf '%02x.json' "$i")
    [ -s "$dir/$f" ] || curl -sSfL -o "$dir/$f" "$base/$f"
    i=$((i + 1))
done
echo "HARTE_DIR=$dir"
