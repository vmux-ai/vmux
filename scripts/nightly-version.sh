#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 4 ]]; then
    echo "usage: $0 <base-version> <yyyymmdd> <run-number> <run-attempt>" >&2
    exit 2
fi

base="$1"
date="$2"
run_number="$3"
run_attempt="$4"

if [[ ! "$base" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
    echo "base version must be major.minor.patch: $base" >&2
    exit 1
fi

major="${BASH_REMATCH[1]}"
minor="${BASH_REMATCH[2]}"
patch="${BASH_REMATCH[3]}"

if [[ ! "$date" =~ ^[0-9]{8}$ ]]; then
    echo "nightly date must be YYYYMMDD: $date" >&2
    exit 1
fi

for value in "$run_number" "$run_attempt"; do
    if [[ ! "$value" =~ ^(0|[1-9][0-9]*)$ ]]; then
        echo "nightly version identifier must be numeric without leading zeroes: $value" >&2
        exit 1
    fi
done

printf '%s.%s.%s-nightly.%s.%s.%s\n' \
    "$major" "$minor" "$((10#$patch + 1))" "$date" "$run_number" "$run_attempt"
