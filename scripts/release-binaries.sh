#!/usr/bin/env bash
# Prebuilt release binaries for a signed release tag (TIN-6749).
#
#   release-binaries.sh build  TAG [DARWIN_SSH_HOST]  build both binaries into dist/TAG/
#   release-binaries.sh sign   TAG [SIGNING_KEY]      write and check dist/TAG/SHA256SUMS.asc
#   release-binaries.sh upload TAG                    attach dist/TAG/* to the GitHub release
#
# Attended, maintainer-only, after the signed tag is pushed. CI cannot do
# this: there is no Darwin runner, and the release key is not in CI.
#
# - Both binaries are the flake's `swb-release` output, built from the
#   tagged commit on GitHub (never from a local worktree): x86_64-linux on
#   this machine, aarch64-darwin over ssh on a Darwin builder.
# - `build` refuses a tag that is not signed by the pinned release key.
# - `sign` signs SHA256SUMS with the release key (gpg picks its signing
#   subkey), then verifies the signature against
#   docs/releases/release-signers.asc exactly as scripts/install.sh does.
set -euo pipefail

REPO="xoxd-ai/agent-switchboard"
SIGNER_FPR="161895136D2E5C292D2A663D0B01977B8DD5DA60"

die() { echo "release-binaries: $*" >&2; exit 1; }
say() { echo "release-binaries: $*" >&2; }

cmd="${1:-}"
tag="${2:-}"
[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "usage: $0 build|sign|upload vMAJOR.MINOR.PATCH [...]"
version="${tag#v}"
root="$(git rev-parse --show-toplevel)"
dist="$root/dist/$tag"

tag_commit() {
  git -C "$root" rev-parse --verify --quiet "refs/tags/$tag^{commit}" \
    || die "tag $tag is not in this clone (git fetch upstream --tags)"
}

check_tag_signer() {
  local primary
  primary="$(git -C "$root" verify-tag --raw "$tag" 2>&1 | awk '$2 == "VALIDSIG" { print $NF }' | head -n 1)"
  [[ "$primary" == "$SIGNER_FPR" ]] || die "tag $tag is not signed by the release key $SIGNER_FPR"
}

build() {
  local darwin_host="${1:-jsullivan2@petting-zoo-mini.local}" sha ref out linux darwin_path got
  [[ "$(uname -s)/$(uname -m)" == "Linux/x86_64" ]] || die "build runs on x86_64-linux (sting or honey)"
  sha="$(tag_commit)"
  check_tag_signer
  ref="github:$REPO/$sha"
  rm -rf "$dist"
  mkdir -p "$dist"

  say "x86_64-linux: nix build $ref#packages.x86_64-linux.swb-release"
  out="$(nix build --no-link --print-out-paths --max-jobs 2 "$ref#packages.x86_64-linux.swb-release")"
  linux="$dist/swb-$tag-x86_64-linux"
  install -m 0755 "$out/bin/swb" "$linux"
  file -L "$linux" | grep -q 'statically linked' || die "$linux is not statically linked"
  got="$("$linux" version)"
  [[ "$got" == "swb $version "* ]] || die "$linux reports '$got', not swb $version"

  say "aarch64-darwin: nix build on $darwin_host"
  # shellcheck disable=SC2029 # ref is meant to expand locally
  darwin_path="$(ssh "$darwin_host" "nix build --no-link --print-out-paths --max-jobs 1 '$ref#packages.aarch64-darwin.swb-release'" | tail -n 1)"
  [[ "$darwin_path" == /nix/store/* ]] || die "Darwin build returned '$darwin_path'"
  # shellcheck disable=SC2029 # the store path is meant to expand locally
  if ssh "$darwin_host" "otool -L '$darwin_path/bin/swb' | tail -n +2" | grep -q /nix/store; then
    die "the Darwin binary links /nix/store"
  fi
  # shellcheck disable=SC2029
  got="$(ssh "$darwin_host" "'$darwin_path/bin/swb' version")"
  [[ "$got" == "swb $version "* ]] || die "the Darwin binary reports '$got', not swb $version"
  scp -q "$darwin_host:$darwin_path/bin/swb" "$dist/swb-$tag-aarch64-darwin"
  chmod 0755 "$dist/swb-$tag-aarch64-darwin"

  git -C "$root" show "$sha:scripts/install.sh" > "$dist/install.sh"
  (cd "$dist" && sha256sum "swb-$tag-x86_64-linux" "swb-$tag-aarch64-darwin" install.sh > SHA256SUMS)
  say "built $dist from $sha:"
  cat "$dist/SHA256SUMS" >&2
}

sign() {
  local key="${1:-$SIGNER_FPR}" sha check primary
  sha="$(tag_commit)"
  [[ -f "$dist/SHA256SUMS" ]] || die "run build first: $dist/SHA256SUMS is missing"
  rm -f "$dist/SHA256SUMS.asc"
  gpg --batch --yes --armor --detach-sign --local-user "$key" --output "$dist/SHA256SUMS.asc" "$dist/SHA256SUMS"
  check="$(mktemp -d)"
  trap 'rm -rf "$check"' RETURN
  chmod 0700 "$check"
  git -C "$root" show "$sha:docs/releases/release-signers.asc" > "$check/release-signers.asc"
  GNUPGHOME="$check" gpg --batch --quiet --import "$check/release-signers.asc" 2>/dev/null
  primary="$(GNUPGHOME="$check" gpg --batch --status-fd 1 --verify "$dist/SHA256SUMS.asc" "$dist/SHA256SUMS" 2>/dev/null \
    | awk '$2 == "VALIDSIG" { print $NF }' | head -n 1)"
  [[ "$primary" == "$SIGNER_FPR" ]] || die "SHA256SUMS.asc does not verify against release-signers.asc as $SIGNER_FPR"
  (cd "$dist" && sha256sum -c SHA256SUMS >&2)
  say "signed $dist/SHA256SUMS.asc ($SIGNER_FPR)"
}

upload() {
  [[ -f "$dist/SHA256SUMS.asc" ]] || die "run build and sign first"
  gh release view "$tag" -R "$REPO" >/dev/null 2>&1 \
    || die "no GitHub release $tag yet; create it with its notes first (gh release create)"
  gh release upload "$tag" -R "$REPO" --clobber \
    "$dist/swb-$tag-x86_64-linux" "$dist/swb-$tag-aarch64-darwin" \
    "$dist/install.sh" "$dist/SHA256SUMS" "$dist/SHA256SUMS.asc"
  say "uploaded to https://github.com/$REPO/releases/tag/$tag"
}

shift 2
case "$cmd" in
  build) build "$@" ;;
  sign) sign "$@" ;;
  upload) upload ;;
  *) die "unknown command '$cmd' (build, sign or upload)" ;;
esac
