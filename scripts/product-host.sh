#!/usr/bin/env bash

vmux_host_read() {
    local product_root="$1"
    local product_key product_value product_variables
    VMUX_HOST_JSON="$("$product_root/scripts/vmux-builder" host-config)" || return
    export VMUX_HOST_JSON
    local product_keys=(
        VMUX_HOST_NAME VMUX_HOST_NAMESPACE VMUX_HOST_DATA_NAME VMUX_HOST_CONFIG_NAME \
        VMUX_HOST_SERVICE_NAME VMUX_HOST_SERVICE_BUNDLE VMUX_HOST_BUNDLE_ID \
        VMUX_HOST_SERVICE_ID VMUX_HOST_AGENT_PLIST VMUX_HOST_CLI_ID \
        VMUX_HOST_KEYCHAIN_SERVICE VMUX_HOST_KEYCHAIN_LABEL VMUX_HOST_RUNTIME_STEM \
        VMUX_HOST_ICON VMUX_HOST_TRAY_ICON VMUX_HOST_ENTITLEMENTS \
        VMUX_HOST_DEV_ENTITLEMENTS VMUX_HOST_USAGE_DESCRIPTIONS VMUX_HOST_DMG_BACKGROUND \
        VMUX_HOST_STABLE_ENDPOINT VMUX_HOST_PREVIEW_ENDPOINT \
        VMUX_HOST_CAMERA VMUX_HOST_MICROPHONE VMUX_HOST_SCREEN
    )
    product_variables="$(mktemp)" || return
    if ! python3 -c 'import json,sys
metadata=json.load(sys.stdin)
output=bytearray()
for key in sys.argv[1:]:
    value=metadata[key]
    if not isinstance(value,str) or "\0" in value:
        raise ValueError("Invalid host metadata value")
    output.extend(key.encode()+b"\0"+value.encode()+b"\0")
sys.stdout.buffer.write(output)' "${product_keys[@]}" <<< "$VMUX_HOST_JSON" > "$product_variables"; then
        rm -f "$product_variables"
        return 1
    fi
    while IFS= read -r -d '' product_key && IFS= read -r -d '' product_value; do
        printf -v "$product_key" '%s' "$product_value"
        export "$product_key"
    done < "$product_variables"
    rm -f "$product_variables"
}
