#!/bin/sh
# Fetch Klaus Dormann's 6502 functional test (GPL-3.0) for local testing only.
# Never commit these files. Usage: sh scripts/fetch-klaus.sh [dir]
set -eu
dir="${1:-target/test-data/klaus}"
mkdir -p "$dir"
base=https://raw.githubusercontent.com/Klaus2m5/6502_65C02_functional_tests/master/bin_files
for f in 6502_functional_test.bin 6502_functional_test.lst; do
    [ -s "$dir/$f" ] || curl -sSfL -o "$dir/$f" "$base/$f"
done
echo "KLAUS_DIR=$dir"
