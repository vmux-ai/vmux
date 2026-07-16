#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" != "3" ]]; then
    echo "usage: $0 <staging-target> <source-target> <destination-target>" >&2
    exit 1
fi

staging_target="$1"
source_target="$2"
destination_target="$3"
source_targets=("$source_target")
cef_build_directories=()
cef_fingerprint_directories=()
shopt -s nullglob

if [[ "$source_target" == "$destination_target" ]]; then
    exit 0
fi

append_source_target() {
    local candidate="$1"
    local existing
    if [[ -z "$candidate" || "$candidate" == "$destination_target" ]]; then
        return
    fi
    for existing in "${source_targets[@]}"; do
        if [[ "$existing" == "$candidate" ]]; then
            return
        fi
    done
    source_targets+=("$candidate")
}

for directory in \
    "$staging_target"/*/build/cef-dll-sys-* \
    "$staging_target"/*/*/build/cef-dll-sys-*
do
    [[ ! -d "$directory" ]] || cef_build_directories+=("$directory")
done

for directory in \
    "$staging_target"/*/.fingerprint/cef-dll-sys-* \
    "$staging_target"/*/*/.fingerprint/cef-dll-sys-*
do
    [[ ! -d "$directory" ]] || cef_fingerprint_directories+=("$directory")
done

for directory in "${cef_build_directories[@]}"; do
    cache="$directory/out/build/CMakeCache.txt"
    if [[ ! -f "$cache" ]]; then
        continue
    fi
    cache_directory="$(sed -n 's/^CMAKE_CACHEFILE_DIR:INTERNAL=//p' "$cache" | tail -1)"
    relative_cache="${cache#"$staging_target/"}"
    relative_build_directory="${relative_cache%/CMakeCache.txt}"
    suffix="/$relative_build_directory"
    if [[ -n "$cache_directory" && "$cache_directory" == *"$suffix" ]]; then
        append_source_target "${cache_directory%"$suffix"}"
    fi
done

rewrite_file() {
    local path="$1"
    local candidate
    for candidate in "${source_targets[@]}"; do
        if LC_ALL=C grep -Fq "$candidate" "$path"; then
            SOURCE_TARGET="$candidate" DESTINATION_TARGET="$destination_target" \
                perl -0pi -e 's/\Q$ENV{SOURCE_TARGET}\E/$ENV{DESTINATION_TARGET}/g' "$path"
        fi
    done
}

for directory in "${cef_fingerprint_directories[@]}"; do
    while IFS= read -r -d '' path; do
        rewrite_file "$path"
    done < <(find "$directory" -type f -name '*.json' -print0)
done

for directory in "${cef_build_directories[@]}"; do
    while IFS= read -r -d '' path; do
        rewrite_file "$path"
    done < <(
        find "$directory" -maxdepth 1 -type f \
            \( -name '*.d' -o -name 'output' -o -name 'root-output' -o -name 'stderr' \) \
            -print0
        if [[ -d "$directory/out/build" ]]; then
            find "$directory/out/build" -type f \
            \( -name '*.cmake' -o -name '*.d' -o -name '*.json' -o -name '*.ninja' \
                -o -name '*.txt' -o -name '*.yaml' \) -print0
        fi
    )
done

for path in \
    "$staging_target"/*/deps/cef_dll_sys-*.d \
    "$staging_target"/*/deps/libcef_dll_sys-*.d \
    "$staging_target"/*/*/deps/cef_dll_sys-*.d \
    "$staging_target"/*/*/deps/libcef_dll_sys-*.d
do
    [[ ! -f "$path" ]] || rewrite_file "$path"
done
