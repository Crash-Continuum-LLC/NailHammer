#!/usr/bin/env bash
#
# Install `nh`, the NailHammer command line tool.
#
#     ./install.sh                 # prebuilt binary if there is one, source if not
#     ./install.sh --from-source   # always build
#     ./install.sh --version v0.2.0
#     ./install.sh --prefix /usr/local/bin
#
# The default path needs no Rust toolchain and no GitHub account: it takes a
# prebuilt `nh` from a release over plain HTTP. `gh` is used only as a fallback,
# which matters while a release is still private -- an authenticated download is
# the one thing curl cannot do on its own.
#
# Falls back to building from source when there is no prebuilt binary for the
# platform, or no way to reach the release. Either way the result is one binary
# on PATH.

set -euo pipefail

REPO="Crash-Continuum-LLC/NailHammer"
MSRV="1.85"

# ~/.local/bin is the default because it needs no sudo. It is in the default
# PATH on most Linux distributions and on none of macOS, so the PATH check at
# the end is not optional politeness -- it is the difference between this
# working and appearing to work.
PREFIX="${NH_INSTALL_PREFIX:-$HOME/.local/bin}"
TAG=""
FROM_SOURCE=0

# Colour only when someone is watching. Piped into a log it is noise.
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  BOLD=$'\033[1m'; RED=$'\033[31m'; YELLOW=$'\033[33m'; GREEN=$'\033[32m'; DIM=$'\033[2m'; OFF=$'\033[0m'
else
  BOLD=""; RED=""; YELLOW=""; GREEN=""; DIM=""; OFF=""
fi

info() { printf '%s==>%s %s\n' "$BOLD" "$OFF" "$*"; }
warn() { printf '%swarning:%s %s\n' "$YELLOW" "$OFF" "$*" >&2; }
die()  { printf '%serror:%s %s\n' "$RED" "$OFF" "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

usage() {
  cat <<EOF
Install nh, the NailHammer command line tool.

USAGE:
    ./install.sh [OPTIONS]

OPTIONS:
    --prefix DIR      install into DIR (default: \$HOME/.local/bin)
    --version TAG     install a specific release, e.g. v0.2.0 (default: latest)
    --from-source     build with cargo instead of taking a prebuilt binary
    -h, --help        show this

ENVIRONMENT:
    NH_INSTALL_PREFIX   same as --prefix
    NO_COLOR            disable colour

Windows is not covered by this script. Take nh-windows-x86_64.zip from a
release, or run this under WSL.
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --prefix)      [ $# -ge 2 ] || die "--prefix needs a directory"; PREFIX="$2"; shift 2 ;;
    --prefix=*)    PREFIX="${1#*=}"; shift ;;
    --version)     [ $# -ge 2 ] || die "--version needs a tag, e.g. v0.2.0"; TAG="$2"; shift 2 ;;
    --version=*)   TAG="${1#*=}"; shift ;;
    --from-source) FROM_SOURCE=1; shift ;;
    -h|--help)     usage; exit 0 ;;
    *)             usage >&2; die "unknown option: $1" ;;
  esac
done

# --- what are we on? ---------------------------------------------------------

# Maps to the labels the release workflow packages. An empty ASSET means there
# is no prebuilt binary for this platform and source is the only route -- the
# script says so rather than failing at download time.
os="$(uname -s)"
arch="$(uname -m)"
case "$os $arch" in
  "Darwin arm64")          ASSET="nh-macos-arm64"   ;;
  "Darwin x86_64")         ASSET="nh-macos-x86_64"  ;;
  "Linux x86_64")          ASSET="nh-linux-x86_64"  ;;
  "Linux aarch64"|"Linux arm64") ASSET="" ;;
  MINGW*|MSYS*|CYGWIN*)    die "Windows is not supported by this script -- see --help" ;;
  *)                       ASSET="" ;;
esac

# --- fetching ----------------------------------------------------------------

# curl on macOS, wget on the minimal Linux images that ship without curl.
fetch() {
  if   have curl; then curl -fsSL "$1"
  elif have wget; then wget -qO- "$1"
  else return 127
  fi
}

# The release API rather than a hardcoded version, so this does not go stale the
# way a documented tag does. Parsed with sed because requiring jq to install a
# tool would be its own small dependency problem.
latest_tag() {
  fetch "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
    | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -1
}

gh_ready() { have gh && gh auth status >/dev/null 2>&1; }

# --- install routes ----------------------------------------------------------

# One temporary directory, removed however we leave.
WORK=""
cleanup() { [ -n "$WORK" ] && rm -rf "$WORK"; }
trap cleanup EXIT

# Returns non-zero rather than exiting, so the caller can fall back to source.
install_prebuilt() {
  WORK="$(mktemp -d)"

  local tag="$TAG"
  if [ -z "$tag" ]; then
    tag="$(latest_tag)" || true
    if [ -z "$tag" ] && gh_ready; then
      tag="$(gh release view --repo "$REPO" --json tagName --jq .tagName 2>/dev/null)" || true
    fi
    [ -n "$tag" ] || { warn "could not find the latest release of $REPO"; return 1; }
  fi

  local archive="$WORK/$ASSET.tar.gz"
  info "Downloading $ASSET from $tag"

  # Plain HTTP first. It needs no account and no tooling beyond curl, which is
  # the whole reason this is the default route.
  if ! fetch "https://github.com/$REPO/releases/download/$tag/$ASSET.tar.gz" > "$archive" 2>/dev/null \
     || [ ! -s "$archive" ]; then
    # A private release answers 404 to an anonymous request, which is
    # indistinguishable from a missing asset. `gh` carries a login, so if one
    # is present it is worth a second attempt before giving up.
    if gh_ready; then
      rm -f "$archive"
      gh release download "$tag" --repo "$REPO" --pattern "$ASSET.tar.gz" --dir "$WORK" 2>/dev/null \
        || { warn "no asset $ASSET.tar.gz on $tag"; return 1; }
    else
      warn "could not download $ASSET.tar.gz from $tag"
      return 1
    fi
  fi

  tar xzf "$archive" -C "$WORK" || { warn "the archive did not unpack"; return 1; }

  # The tarball holds a directory of the same name containing the binary
  # alongside the docs. Only the binary is installed; the docs are already in
  # the repository and a second stale copy in ~/.local/bin helps nobody.
  local built="$WORK/$ASSET/nh"
  [ -f "$built" ] || { warn "the archive did not contain $ASSET/nh"; return 1; }

  place "$built"
}

install_from_source() {
  have cargo || die \
    "cargo not found. Install Rust from https://rustup.rs, or use a prebuilt binary."

  # A version older than the workspace MSRV fails deep in a dependency build
  # with an error that does not mention the toolchain. Saying it up front costs
  # one command and several minutes of confusion.
  local have_ver
  have_ver="$(cargo --version | awk '{print $2}')"
  if [ "$(printf '%s\n%s\n' "$MSRV" "$have_ver" | sort -V | head -1)" != "$MSRV" ]; then
    die "Rust $have_ver is too old -- NailHammer needs $MSRV or newer. Run: rustup update"
  fi

  # Prefer the checkout this script lives in. Building from the local path needs
  # no network, and installs exactly the tree being read rather than whatever is
  # on the default branch.
  local here
  here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

  if [ -f "$here/crates/nh-cli/Cargo.toml" ]; then
    info "Building from $here (this takes a few minutes)"
    cargo install --path "$here/crates/nh-cli" --locked --force --root "${PREFIX%/bin}" \
      || die "the build failed"
  else
    info "Building from $REPO (this takes a few minutes)"
    cargo install --git "https://github.com/$REPO" nh-cli --locked --force --root "${PREFIX%/bin}" \
      || die "the build failed"
  fi

  # --root puts the binary in <root>/bin, which is already PREFIX when PREFIX
  # ends in /bin. When it does not, cargo has just put it one level down -- and
  # has also printed its own "be sure to add <root>/bin to your PATH" advice
  # naming a directory this script is about to empty. Move it and say where it
  # actually landed, so the last word on the subject is the true one.
  if [ "${PREFIX%/bin}" != "$PREFIX" ]; then
    info "Installed ${BOLD}$PREFIX/nh${OFF}"
    return 0
  fi
  place "$PREFIX/bin/nh"
  rmdir "$PREFIX/bin" 2>/dev/null || true
}

# Put a binary at $PREFIX/nh, atomically enough that a failure never leaves a
# half-written file where a working one used to be.
place() {
  local from="$1" to="$PREFIX/nh"

  if [ -e "$to" ] && [ ! -w "$to" ]; then
    die "$to exists and is not writable -- rerun with --prefix, or: sudo ./install.sh --prefix $PREFIX"
  fi

  chmod +x "$from"

  # Downloads can carry the quarantine flag, and on macOS a quarantined binary
  # is refused by Gatekeeper with a dialog rather than an error on stdout.
  if [ "$os" = "Darwin" ] && have xattr; then
    xattr -d com.apple.quarantine "$from" 2>/dev/null || true
  fi

  mv -f "$from" "$to"
  info "Installed ${BOLD}$to${OFF}"
}

# --- run it ------------------------------------------------------------------

mkdir -p "$PREFIX" || die "could not create $PREFIX"
[ -w "$PREFIX" ] || die "$PREFIX is not writable -- rerun with --prefix DIR, or with sudo"

if [ "$FROM_SOURCE" -eq 1 ]; then
  install_from_source
elif [ -z "$ASSET" ]; then
  warn "no prebuilt binary for $os $arch -- building from source"
  install_from_source
elif ! install_prebuilt; then
  warn "falling back to building from source"
  install_from_source
fi

# --- did it work? ------------------------------------------------------------

[ -x "$PREFIX/nh" ] || die "nothing was installed at $PREFIX/nh"

# Run the binary that was just installed, by full path. Asking PATH which `nh`
# to run would report success from some older copy elsewhere.
version="$("$PREFIX/nh" --version 2>/dev/null)" || die \
  "$PREFIX/nh was installed but will not run"

printf '%s%s%s\n' "$GREEN" "$version" "$OFF"

# A binary that is not on PATH is not installed as far as the user is concerned,
# so this reports the exact line to add rather than "make sure it is on PATH".
case ":${PATH}:" in
  *":$PREFIX:"*) ;;
  *)
    case "${SHELL##*/}" in
      zsh)  rc="~/.zshrc"  ;;
      bash) rc="~/.bashrc" ;;
      fish) rc="~/.config/fish/config.fish" ;;
      *)    rc="your shell's startup file" ;;
    esac
    echo
    warn "$PREFIX is not on your PATH."
    if [ "${SHELL##*/}" = "fish" ]; then
      printf '  Add to %s:\n\n    %sfish_add_path %s%s\n\n' "$rc" "$DIM" "$PREFIX" "$OFF"
    else
      printf '  Add to %s:\n\n    %sexport PATH="%s:$PATH"%s\n\n' "$rc" "$DIM" "$PREFIX" "$OFF"
    fi
    printf '  Then restart the shell, or run it now to use nh in this one.\n'
    ;;
esac

echo
echo "Next:"
printf '  %snh init my-language%s     scaffold a project\n' "$DIM" "$OFF"
printf '  %snh --help%s               everything else\n' "$DIM" "$OFF"
