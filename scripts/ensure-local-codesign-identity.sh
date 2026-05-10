#!/usr/bin/env bash
set -euo pipefail

IDENTITY_NAME="${VMUX_LOCAL_SIGNING_IDENTITY:-Vmux Dev}"
KEYCHAIN="${VMUX_LOCAL_CODESIGN_KEYCHAIN:-$(security default-keychain -d user | awk -F'"' '/"/ { print $2; exit }')}"

case "$IDENTITY_NAME" in
*$'\n'* | *$'\r'*)
    echo "Error: VMUX_LOCAL_SIGNING_IDENTITY must be a single line." >&2
    exit 1
    ;;
esac

if [[ -z "$KEYCHAIN" ]]; then
    echo "Error: default user keychain not found." >&2
    exit 1
fi

if [[ -f "$KEYCHAIN" ]]; then
    security list-keychains -d user -s "$KEYCHAIN" >/dev/null
fi

find_identity() {
    security find-identity -v -p codesigning "$KEYCHAIN" | grep -Fq "\"$IDENTITY_NAME\""
}

if find_identity; then
    printf '%s\n' "$IDENTITY_NAME"
    exit 0
fi

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/vmux-local-codesign.XXXXXX")"
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

OPENSSL_CONF="$TMP_DIR/openssl.cnf"
CERT_FILE="$TMP_DIR/cert.pem"
KEY_FILE="$TMP_DIR/key.pem"
P12_FILE="$TMP_DIR/identity.p12"
P12_PASSWORD="$(uuidgen 2>/dev/null || printf 'vmux-local-codesign')"

cat > "$OPENSSL_CONF" <<EOF
[req]
distinguished_name=req_distinguished_name
x509_extensions=v3_codesign
prompt=no
[req_distinguished_name]
CN=$IDENTITY_NAME
[v3_codesign]
basicConstraints=critical,CA:false
keyUsage=critical,digitalSignature
extendedKeyUsage=critical,codeSigning
subjectKeyIdentifier=hash
authorityKeyIdentifier=keyid:always
EOF

cat >&2 <<EOF
==> Creating local codesigning identity: $IDENTITY_NAME
    The following will be added to your default keychain ($KEYCHAIN):
      - a self-signed RSA-2048 certificate (CN=$IDENTITY_NAME, valid 10y)
      - the matching private key (accessible by /usr/bin/codesign)
      - a codesigning trust anchor for the certificate above
    Effect: any binary signed locally with this identity will be trusted
    for codesigning on this user account. The private key never leaves
    this machine.
    If macOS prompts for Keychain access, use Touch ID when offered.
EOF

openssl req \
    -new \
    -newkey rsa:2048 \
    -nodes \
    -x509 \
    -days 3650 \
    -sha256 \
    -config "$OPENSSL_CONF" \
    -keyout "$KEY_FILE" \
    -out "$CERT_FILE" >/dev/null 2>&1

# macOS `security import` cannot read PKCS#12 produced with OpenSSL 3's
# default AES-256-CBC PBE. Force legacy SHA1/3DES so the import succeeds.
openssl pkcs12 \
    -export \
    -out "$P12_FILE" \
    -inkey "$KEY_FILE" \
    -in "$CERT_FILE" \
    -passout "pass:$P12_PASSWORD" \
    -keypbe PBE-SHA1-3DES \
    -certpbe PBE-SHA1-3DES \
    -macalg sha1 >/dev/null 2>&1

security import "$P12_FILE" \
    -f pkcs12 \
    -k "$KEYCHAIN" \
    -P "$P12_PASSWORD" \
    -T /usr/bin/codesign >/dev/null

security add-trusted-cert \
    -r trustRoot \
    -p codeSign \
    -k "$KEYCHAIN" \
    "$CERT_FILE" >/dev/null

if ! security set-key-partition-list \
    -S apple-tool:,apple:,codesign: \
    -s \
    -l "$IDENTITY_NAME" \
    "$KEYCHAIN" >/dev/null; then
    echo "Warning: could not pre-authorize codesign key access; macOS may ask once during signing." >&2
fi

if ! find_identity; then
    echo "Error: local codesigning identity was created but is not valid for codesigning." >&2
    exit 1
fi

printf '%s\n' "$IDENTITY_NAME"
