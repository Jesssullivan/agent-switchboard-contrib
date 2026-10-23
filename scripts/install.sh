#!/bin/sh
# Install a prebuilt swb release binary (TIN-6749).
#
#   sh install.sh [VERSION]        # e.g. v0.3.0; default: the latest release
#
# Downloads swb-VERSION-<system> and SHA256SUMS from the GitHub release,
# verifies the checksum (always), verifies SHA256SUMS.asc against the pinned
# release signer when gpg is available, and installs to ~/.local/bin/swb.
#
# Settings (environment):
#   SWB_INSTALL_DIR         install directory (default: $HOME/.local/bin)
#   SWB_REQUIRE_SIGNATURE=1 fail instead of warning when gpg is missing
#   SWB_RELEASE_BASE        release download base (default: GitHub releases)
#
# Systems with a prebuilt binary: x86_64-linux (static, musl) and
# aarch64-darwin. Anything else: nix profile install or cargo, see README.
set -eu

REPO="xoxd-ai/agent-switchboard"
# Primary fingerprint of the operator release key in
# docs/releases/release-signers.asc, the only key that signs releases.
SIGNER_FPR="161895136D2E5C292D2A663D0B01977B8DD5DA60"
BASE="${SWB_RELEASE_BASE:-https://github.com/$REPO/releases}"
DEST="${SWB_INSTALL_DIR:-${HOME:?HOME is not set}/.local/bin}"

die() { echo "install.sh: $*" >&2; exit 1; }
say() { echo "install.sh: $*" >&2; }

command -v curl >/dev/null 2>&1 || die "curl is required"

version="${1:-}"
if [ -z "$version" ]; then
  # The latest release's tag, from the redirect GitHub answers with.
  url="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$BASE/latest")" \
    || die "could not resolve the latest release"
  version="${url##*/}"
fi
case "$version" in
  v[0-9]*.[0-9]*.[0-9]*) ;;
  *) die "version must look like v0.3.0, got '$version'" ;;
esac

os="$(uname -s)"
arch="$(uname -m)"
case "$os/$arch" in
  Linux/x86_64 | Linux/amd64) system=x86_64-linux ;;
  Darwin/arm64 | Darwin/aarch64) system=aarch64-darwin ;;
  *) die "no prebuilt binary for $os/$arch; use 'nix profile install github:$REPO/$version#swb' or build with cargo (README → Install)" ;;
esac

asset="swb-$version-$system"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT INT TERM

fetch() { curl -fsSL --proto '=https' --retry 2 -o "$work/$2" "$1" || die "download failed: $1"; }

say "downloading $asset ($version)"
fetch "$BASE/download/$version/$asset" "$asset"
fetch "$BASE/download/$version/SHA256SUMS" SHA256SUMS

# 1. Signature over SHA256SUMS, when gpg is available.
if command -v gpg >/dev/null 2>&1; then
  fetch "$BASE/download/$version/SHA256SUMS.asc" SHA256SUMS.asc
  fetch "https://raw.githubusercontent.com/$REPO/$version/docs/releases/release-signers.asc" release-signers.asc
  GNUPGHOME="$work/gnupg"
  export GNUPGHOME
  mkdir -m 0700 "$GNUPGHOME"
  gpg --batch --quiet --import "$work/release-signers.asc" 2>/dev/null \
    || die "could not import release-signers.asc"
  # Trust comes from the pinned fingerprint, not from the downloaded key
  # file: VALIDSIG's last field is the signing key's primary fingerprint.
  status="$(gpg --batch --status-fd 1 --verify "$work/SHA256SUMS.asc" "$work/SHA256SUMS" 2>/dev/null || true)"
  primary="$(printf '%s\n' "$status" | awk '$2 == "VALIDSIG" { print $NF }' | head -n 1)"
  [ "$primary" = "$SIGNER_FPR" ] \
    || die "SHA256SUMS.asc is not a valid signature by $SIGNER_FPR"
  say "signature ok: SHA256SUMS signed by $SIGNER_FPR"
elif [ "${SWB_REQUIRE_SIGNATURE:-0}" = 1 ]; then
  die "gpg is not installed and SWB_REQUIRE_SIGNATURE=1"
else
  say "WARNING: gpg not found; checking the SHA-256 checksum only, not the signature"
fi

# 2. Checksum of the binary against its SHA256SUMS line.
expected="$(awk -v f="$asset" '$2 == f || $2 == "*" f { print $1 }' "$work/SHA256SUMS")"
[ -n "$expected" ] || die "$asset is not listed in SHA256SUMS"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$work/$asset" | awk '{ print $1 }')"
elif command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "$work/$asset" | awk '{ print $1 }')"
else
  die "need sha256sum or shasum to verify the download"
fi
[ "$actual" = "$expected" ] || die "checksum mismatch for $asset: expected $expected, got $actual"
say "checksum ok: $actual"

# 3. Install.
mkdir -p "$DEST"
chmod 0755 "$work/$asset"
mv -f "$work/$asset" "$DEST/swb"
say "installed $DEST/swb"
"$DEST/swb" version || die "the installed binary did not run"
case ":$PATH:" in
  *":$DEST:"*) ;;
  *) say "note: $DEST is not on PATH; add it, e.g. export PATH=\"$DEST:\$PATH\"" ;;
esac
