#!/usr/bin/env bash
set -euo pipefail

# Promote the build scripts/build-ios-release.sh just uploaded to App Store review.
#
# altool can only upload; attaching a build to a version and submitting it is App Store Connect
# API work, so this talks to the REST API directly with a JWT signed by an ASC API key. No Xcode
# and no macOS runner needed — it is curl and openssl.
#
# Dormant unless APPLE_ASC_KEY_ID / APPLE_ASC_ISSUER_ID / APPLE_ASC_PRIVATE_KEY are all set. That
# is deliberate: submission cannot succeed until the app record, metadata, screenshots and privacy
# answers exist in App Store Connect (VMX-139), and a job that is red on every release teaches
# people to ignore it.
#
# Required to do anything:
#   APPLE_ASC_KEY_ID        the API key's ten-character id
#   APPLE_ASC_ISSUER_ID     the issuer UUID from Users and Access > Integrations
#   APPLE_ASC_PRIVATE_KEY   contents of the AuthKey_XXXX.p8
#   VMUX_IOS_BUILD_NUMBER   the CFBundleVersion that was uploaded
# Optional:
#   VMUX_IOS_SUBMIT_TIMEOUT seconds to wait for Apple to finish processing (default 1800)
#
# Usage: ./scripts/submit-ios-release.sh

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

BUNDLE_ID="ai.vmux.mobile"
API="https://api.appstoreconnect.apple.com"

if [[ -z "${APPLE_ASC_KEY_ID:-}" || -z "${APPLE_ASC_ISSUER_ID:-}" || -z "${APPLE_ASC_PRIVATE_KEY:-}" ]]; then
    echo "==> App Store Connect API key not configured; skipping submission."
    echo "    The build is in TestFlight. Submit it by hand, or set APPLE_ASC_KEY_ID,"
    echo "    APPLE_ASC_ISSUER_ID and APPLE_ASC_PRIVATE_KEY to automate this."
    exit 0
fi

: "${VMUX_IOS_BUILD_NUMBER:?missing VMUX_IOS_BUILD_NUMBER}"

VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')"
TIMEOUT="${VMUX_IOS_SUBMIT_TIMEOUT:-1800}"

TMP_DIR="$(mktemp -d -t vmux-ios-submit)"
cleanup() { rm -rf "$TMP_DIR"; }
trap cleanup EXIT

KEY_FILE="$TMP_DIR/key.p8"
printf '%s\n' "$APPLE_ASC_PRIVATE_KEY" > "$KEY_FILE"

b64url() { openssl base64 -A | tr '+/' '-_' | tr -d '='; }

# ES256, hand-rolled because the runners have no jwt tooling and adding a Python or Ruby
# toolchain for one signature is worse than twenty lines of openssl.
mint_token() {
    local now exp header payload signing_input sig_der r s
    now="$(date +%s)"
    exp="$((now + 1200))"
    header="$(printf '{"alg":"ES256","kid":"%s","typ":"JWT"}' "$APPLE_ASC_KEY_ID" | b64url)"
    payload="$(printf '{"iss":"%s","iat":%s,"exp":%s,"aud":"appstoreconnect-v1"}' \
        "$APPLE_ASC_ISSUER_ID" "$now" "$exp" | b64url)"
    signing_input="$header.$payload"

    printf '%s' "$signing_input" | openssl dgst -sha256 -sign "$KEY_FILE" -out "$TMP_DIR/sig.der"

    # openssl emits a DER SEQUENCE of two INTEGERs; JOSE wants raw r||s, each left-padded to 32
    # bytes. asn1parse prints both in hex, with leading zero bytes already stripped.
    # Read in a loop rather than with mapfile — macOS ships bash 3.2, which has no mapfile.
    r=""
    s=""
    while IFS= read -r value; do
        if [[ -z "$r" ]]; then r="$value"; else s="$value"; fi
    done < <(openssl asn1parse -inform DER -in "$TMP_DIR/sig.der" | awk -F':' '/INTEGER/ {print $NF}')
    if [[ -z "$r" || -z "$s" ]]; then
        echo "Error: could not parse the ECDSA signature; is APPLE_ASC_PRIVATE_KEY a valid .p8?" >&2
        exit 1
    fi
    r="$(printf '%064s' "$r" | tr ' ' '0')"
    s="$(printf '%064s' "$s" | tr ' ' '0')"
    printf '%s.%s' "$signing_input" "$(printf '%s%s' "$r" "$s" | xxd -r -p | b64url)"
}

TOKEN="$(mint_token)"

# Fails the script on any non-2xx rather than letting a JSON error body flow downstream as if it
# were data.
api() {
    local method="$1" path="$2" body="${3:-}"
    local args=(-sS -X "$method" -H "Authorization: Bearer $TOKEN"
        -H "Content-Type: application/json" -w '\n%{http_code}')
    [[ -n "$body" ]] && args+=(-d "$body")
    local response status payload
    response="$(curl "${args[@]}" "$API$path")"
    status="$(printf '%s' "$response" | tail -n1)"
    payload="$(printf '%s' "$response" | sed '$d')"
    if [[ "$status" != 2* ]]; then
        echo "Error: $method $path returned $status" >&2
        printf '%s\n' "$payload" | head -40 >&2
        exit 1
    fi
    printf '%s' "$payload"
}

echo "==> Resolving $BUNDLE_ID"
APP_ID="$(api GET "/v1/apps?filter\[bundleId\]=$BUNDLE_ID&limit=1" | jq -r '.data[0].id // empty')"
if [[ -z "$APP_ID" ]]; then
    echo "Error: no app record for $BUNDLE_ID in App Store Connect. Create it first (VMX-139)." >&2
    exit 1
fi

echo "==> Waiting for build $VERSION ($VMUX_IOS_BUILD_NUMBER) to finish processing"
BUILD_ID=""
DEADLINE="$(( $(date +%s) + TIMEOUT ))"
while :; do
    BUILD="$(api GET "/v1/builds?filter\[app\]=$APP_ID&filter\[version\]=$VMUX_IOS_BUILD_NUMBER&limit=1")"
    BUILD_ID="$(printf '%s' "$BUILD" | jq -r '.data[0].id // empty')"
    STATE="$(printf '%s' "$BUILD" | jq -r '.data[0].attributes.processingState // "MISSING"')"
    case "$STATE" in
        VALID)
            echo "    build $BUILD_ID is VALID"
            break
            ;;
        FAILED | INVALID)
            echo "Error: Apple rejected the build during processing (state $STATE)." >&2
            exit 1
            ;;
    esac
    if [[ "$(date +%s)" -ge "$DEADLINE" ]]; then
        echo "Error: build still $STATE after ${TIMEOUT}s. It is uploaded; submit it by hand." >&2
        exit 1
    fi
    echo "    $STATE, waiting..."
    sleep 30
    TOKEN="$(mint_token)"
done

echo "==> Finding or creating the $VERSION App Store version"
VERSION_ID="$(api GET "/v1/apps/$APP_ID/appStoreVersions?filter\[versionString\]=$VERSION&limit=1" \
    | jq -r '.data[0].id // empty')"
if [[ -z "$VERSION_ID" ]]; then
    VERSION_ID="$(api POST "/v1/appStoreVersions" "$(jq -nc \
        --arg v "$VERSION" --arg app "$APP_ID" \
        '{data:{type:"appStoreVersions",attributes:{platform:"IOS",versionString:$v},
          relationships:{app:{data:{type:"apps",id:$app}}}}}')" | jq -r '.data.id')"
    echo "    created $VERSION_ID"
else
    echo "    reusing $VERSION_ID"
fi

echo "==> Attaching the build"
api PATCH "/v1/appStoreVersions/$VERSION_ID/relationships/build" \
    "$(jq -nc --arg id "$BUILD_ID" '{data:{type:"builds",id:$id}}')" >/dev/null

echo "==> Submitting for review"
SUBMISSION_ID="$(api POST "/v1/reviewSubmissions" "$(jq -nc --arg app "$APP_ID" \
    '{data:{type:"reviewSubmissions",attributes:{platform:"IOS"},
      relationships:{app:{data:{type:"apps",id:$app}}}}}')" | jq -r '.data.id')"

api POST "/v1/reviewSubmissionItems" "$(jq -nc --arg sub "$SUBMISSION_ID" --arg ver "$VERSION_ID" \
    '{data:{type:"reviewSubmissionItems",
      relationships:{reviewSubmission:{data:{type:"reviewSubmissions",id:$sub}},
                     appStoreVersion:{data:{type:"appStoreVersions",id:$ver}}}}}')" >/dev/null

api PATCH "/v1/reviewSubmissions/$SUBMISSION_ID" "$(jq -nc --arg id "$SUBMISSION_ID" \
    '{data:{type:"reviewSubmissions",id:$id,attributes:{submitted:true}}}')" >/dev/null

echo "==> Submitted $VERSION ($VMUX_IOS_BUILD_NUMBER) for review as $SUBMISSION_ID"
