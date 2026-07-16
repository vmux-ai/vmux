#!/usr/bin/env bash

vmux_target_lock_file() {
    local target_dir="$1"
    local repo_root="$2"
    local lock_key
    local lock_root

    lock_key="$(printf '%s\n' "$target_dir" | git -C "$repo_root" hash-object --stdin)"
    lock_root="${VMUX_TARGET_LOCK_ROOT:-${TMPDIR:-/tmp}/vmux-target-locks-${UID:-$(id -u)}}"
    if [[ -L "$lock_root" ]]; then
        echo "unsafe target lock root: $lock_root" >&2
        return 1
    fi
    mkdir -p "$lock_root"
    chmod 700 "$lock_root"
    printf '%s/%s.lock\n' "$lock_root" "$lock_key"
}

vmux_target_lock_acquire() {
    local target_dir="$1"
    local repo_root="$2"
    local slot="$3"
    local mode="${4:-wait}"
    local lock_file
    local lock_var="VMUX_TARGET_LOCK_FILE_$slot"
    local waiting=0

    lock_file="$(vmux_target_lock_file "$target_dir" "$repo_root")"
    case "$(uname -s)" in
        Darwin)
            while ! shlock -f "$lock_file" -p "$$"; do
                if [[ "$mode" == "try" ]]; then
                    return 1
                fi
                if [[ "$waiting" == "0" ]]; then
                    echo "waiting for target cache lock: $target_dir"
                    waiting=1
                fi
                sleep 1
            done
            ;;
        Linux)
            case "$slot" in
                8) exec 8>"$lock_file" ;;
                9) exec 9>"$lock_file" ;;
                *) echo "unsupported target lock slot: $slot" >&2; return 1 ;;
            esac
            if [[ "$mode" == "try" ]]; then
                flock -n "$slot" || return 1
            else
                flock "$slot"
            fi
            ;;
        *)
            echo "target cache locking unsupported on $(uname -s)" >&2
            return 1
            ;;
    esac
    printf -v "$lock_var" '%s' "$lock_file"
}

vmux_target_lock_release() {
    local slot="$1"
    local lock_var="VMUX_TARGET_LOCK_FILE_$slot"
    local lock_file="${!lock_var:-}"

    if [[ -z "$lock_file" ]]; then
        return 0
    fi
    case "$(uname -s)" in
        Darwin)
            if [[ -r "$lock_file" ]] && [[ "$(<"$lock_file")" == "$$" ]]; then
                rm -f -- "$lock_file"
            fi
            ;;
        Linux)
            flock -u "$slot"
            case "$slot" in
                8) exec 8>&- ;;
                9) exec 9>&- ;;
            esac
            ;;
    esac
    printf -v "$lock_var" '%s' ""
}
