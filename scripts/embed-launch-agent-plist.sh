#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/product-host.sh"
vmux_host_read "$ROOT"

: "${VMUX_APP_BUNDLE:?VMUX_APP_BUNDLE not set}"
: "${VMUX_BUILD_PROFILE:?VMUX_BUILD_PROFILE not set}"

PLIST_SRC="$ROOT/packaging/macos/ai.vmux.service.plist"
PLIST_DST="$VMUX_APP_BUNDLE/Contents/Library/LaunchAgents/$VMUX_HOST_AGENT_PLIST"
SERVICE_APP="$VMUX_APP_BUNDLE/Contents/Library/LoginItems/$VMUX_HOST_SERVICE_BUNDLE"
SERVICE_SRC="$VMUX_APP_BUNDLE/Contents/MacOS/$VMUX_HOST_SERVICE_NAME"
SERVICE_EXEC="$SERVICE_APP/Contents/MacOS/$VMUX_HOST_SERVICE_NAME"
SERVICE_INFO="$SERVICE_APP/Contents/Info.plist"
SERVICE_ICON="$SERVICE_APP/Contents/Resources/Vmux.icns"
PARENT_BUNDLE_ID="${VMUX_BUNDLE_ID:-$VMUX_HOST_BUNDLE_ID}"

LABEL="$VMUX_HOST_SERVICE_ID"
if [[ "$VMUX_BUILD_PROFILE" == "local" ]]; then
    : "${VMUX_GIT_HASH:?VMUX_GIT_HASH must be set for local builds}"
    LABEL="$VMUX_HOST_SERVICE_ID.$VMUX_GIT_HASH"
elif [[ "$VMUX_BUILD_PROFILE" != release ]]; then
    LABEL="$VMUX_HOST_SERVICE_ID.$VMUX_BUILD_PROFILE"
fi

mkdir -p "$SERVICE_APP/Contents/MacOS" "$SERVICE_APP/Contents/Resources"
if [[ -f "$SERVICE_SRC" ]]; then
    mv -f "$SERVICE_SRC" "$SERVICE_EXEC"
elif [[ ! -f "$SERVICE_EXEC" ]]; then
    echo "Missing service executable: $SERVICE_SRC" >&2
    exit 1
fi

cp -f "$VMUX_HOST_ICON" "$SERVICE_ICON"

mkdir -p "$(dirname "$PLIST_DST")"
python3 - "$SERVICE_INFO" "$PLIST_SRC" "$PLIST_DST" "$LABEL" "$PARENT_BUNDLE_ID" <<'PY'
import json, os, pathlib, plistlib, sys
service_info, template, destination = map(pathlib.Path, sys.argv[1:4])
label, parent = sys.argv[4:6]
metadata = json.loads(os.environ['VMUX_HOST_JSON'])
name = metadata['VMUX_HOST_SERVICE_NAME']
service = dict(CFBundleDevelopmentRegion='en', CFBundleDisplayName=name,
    CFBundleExecutable=name, CFBundleIdentifier=label, CFBundleInfoDictionaryVersion='6.0',
    CFBundleName=name, CFBundlePackageType='APPL', CFBundleIconFile='Vmux', LSUIElement=True)
with service_info.open('wb') as output:
    plistlib.dump(service, output)
with template.open('rb') as source:
    launch = plistlib.load(source)
launch['Label'] = label
launch['BundleProgram'] = 'Contents/Library/LoginItems/'+metadata['VMUX_HOST_SERVICE_BUNDLE']+'/Contents/MacOS/'+name
launch['AssociatedBundleIdentifiers'] = [parent]
launch['EnvironmentVariables']['VMUX_BUILD_PROFILE'] = os.environ['VMUX_BUILD_PROFILE']
log = '/tmp/'+metadata['VMUX_HOST_RUNTIME_STEM']+'-service.log'
launch['StandardOutPath'] = log
launch['StandardErrorPath'] = log
with destination.open('wb') as output:
    plistlib.dump(launch, output)
PY
echo "==> Embedded launchd plist (label=$LABEL, profile=$VMUX_BUILD_PROFILE)"
