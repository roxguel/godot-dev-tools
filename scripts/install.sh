#!/bin/sh
# Installs gdt (Godot Development Tools) on Linux and macOS.
#
#   curl -fsSL https://gdt.roxguel.dev/install.sh | sh
#   curl -fsSL https://gdt.roxguel.dev/install.sh | sh -s -- --version 0.1.0 --dir "$HOME/bin"
#
# It downloads the release archive for this machine from GitHub, checks it
# against the SHA256SUMS file of the same release, and puts the `gdt` executable
# in one folder. It does not change PATH, shell profiles or anything else.
#
# Options (or the environment variables in brackets):
#   --version <v>         install this version, for example 0.1.0   [GDT_VERSION]
#                         default: the latest release
#   --dir <path>          install into this folder                  [GDT_INSTALL_DIR]
#                         default: $HOME/.local/bin
#   --verify-attestation  also check the build provenance with `gh attestation verify`
#                         (needs the GitHub CLI, signed in)
#   -h, --help            show this text
#
# GDT_BASE_URL replaces https://github.com/roxguel/godot-dev-tools (for mirrors and tests).

set -eu

REPO_URL="${GDT_BASE_URL:-https://github.com/roxguel/godot-dev-tools}"
REPO_SLUG="roxguel/godot-dev-tools"

say() { printf '%s\n' "$*"; }
fail() { printf 'gdt installer: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

usage() {
    cat <<'EOF'
Installs gdt (Godot Development Tools).

Options (or the environment variables in brackets):
  --version <v>         install this version, for example 0.1.0   [GDT_VERSION]
                        default: the latest release
  --dir <path>          install into this folder                  [GDT_INSTALL_DIR]
                        default: $HOME/.local/bin
  --verify-attestation  also check the build provenance with `gh attestation verify`
                        (needs the GitHub CLI, signed in)
  -h, --help            show this text

It checks the download against the SHA256SUMS of the release and changes
nothing but the one executable. It does not touch PATH.
EOF
}

# --- network ------------------------------------------------------------------

# When the default GitHub URL is used, nothing but HTTPS is accepted.
proto_curl() { [ -z "${GDT_BASE_URL:-}" ] && printf '%s' "--proto =https --tlsv1.2" || true; }

download() { # url file
    if have curl; then
        # shellcheck disable=SC2046
        curl -fsSL $(proto_curl) -o "$2" "$1" || fail "could not download $1"
    elif have wget; then
        wget -q -O "$2" "$1" || fail "could not download $1"
    else
        fail "curl or wget is needed"
    fi
}

latest_tag() {
    url="$REPO_URL/releases/latest"
    if have curl; then
        # shellcheck disable=SC2046
        final=$(curl -fsSL $(proto_curl) -o /dev/null -w '%{url_effective}' "$url") \
            || fail "could not find the latest release ($url); is there a published release yet?"
    elif have wget; then
        final=$(wget --spider -S --max-redirect=0 "$url" 2>&1 | sed -n 's/^ *[Ll]ocation: *//p' | head -n 1 | tr -d '\r') \
            || true
        [ -n "$final" ] || fail "could not find the latest release ($url); is there a published release yet?"
    else
        fail "curl or wget is needed"
    fi
    tag=${final##*/}
    case "$tag" in
        v[0-9]*) printf '%s' "$tag" ;;
        *) fail "could not read the latest version from $final" ;;
    esac
}

# --- this machine -------------------------------------------------------------

detect_target() {
    os=$(uname -s)
    arch=$(uname -m)
    case "$os" in
        Linux)
            case "$arch" in
                x86_64 | amd64) printf 'x86_64-unknown-linux-musl' ;;
                aarch64 | arm64) printf 'aarch64-unknown-linux-musl' ;;
                *) fail "no build for Linux on $arch" ;;
            esac
            ;;
        Darwin)
            # A shell running under Rosetta reports x86_64 on an Apple silicon Mac.
            if [ "$arch" = "x86_64" ] && [ "$(sysctl -n hw.optional.arm64 2>/dev/null || true)" = "1" ]; then
                arch=arm64
            fi
            case "$arch" in
                arm64 | aarch64) printf 'aarch64-apple-darwin' ;;
                x86_64) printf 'x86_64-apple-darwin' ;;
                *) fail "no build for macOS on $arch" ;;
            esac
            ;;
        *) fail "no build for $os (Windows has install.ps1)" ;;
    esac
}

sha256_of() {
    if have sha256sum; then
        sha256sum "$1" | cut -d ' ' -f 1
    elif have shasum; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    else
        fail "sha256sum or shasum is needed to check the download"
    fi
}

# --- main ---------------------------------------------------------------------

main() {
    version="${GDT_VERSION:-}"
    dir="${GDT_INSTALL_DIR:-}"
    verify_attestation=0
    while [ $# -gt 0 ]; do
        case "$1" in
            --version) [ $# -ge 2 ] || fail "--version needs a value"; version=$2; shift 2 ;;
            --dir) [ $# -ge 2 ] || fail "--dir needs a value"; dir=$2; shift 2 ;;
            --verify-attestation) verify_attestation=1; shift ;;
            -h | --help) usage; exit 0 ;;
            *) fail "unknown option: $1 (see --help)" ;;
        esac
    done

    if [ -z "$dir" ]; then
        [ -n "${HOME:-}" ] || fail "HOME is not set; say where to install with --dir"
        dir="$HOME/.local/bin"
    fi

    if [ -z "$version" ]; then
        tag=$(latest_tag)
    else
        tag="v${version#v}"
    fi
    # The tag ends up in URLs and file names, so it is held to a strict shape.
    case "$tag" in
        v[0-9]*.[0-9]*.[0-9]*) ;;
        *) fail "not a version: $tag" ;;
    esac
    case "$tag" in
        *[!0-9A-Za-z.+-]*) fail "not a version: $tag" ;;
    esac

    target=$(detect_target)
    name="gdt-$tag-$target"
    archive="$name.tar.gz"

    tmp=$(mktemp -d 2>/dev/null || mktemp -d -t gdt-install) || fail "could not create a temporary folder"
    trap 'rm -rf "$tmp"' EXIT INT TERM

    say "Installing gdt $tag for $target"
    download "$REPO_URL/releases/download/$tag/$archive" "$tmp/$archive"
    download "$REPO_URL/releases/download/$tag/SHA256SUMS" "$tmp/SHA256SUMS"

    expected=$(awk -v f="$archive" '{ n = $2; sub(/^\*/, "", n); if (n == f) { print $1; exit } }' "$tmp/SHA256SUMS")
    [ -n "$expected" ] || fail "$archive is not listed in SHA256SUMS; nothing was installed"
    actual=$(sha256_of "$tmp/$archive")
    if [ "$actual" != "$expected" ]; then
        fail "checksum mismatch for $archive (expected $expected, got $actual); nothing was installed"
    fi
    say "Checksum ok"

    if [ "$verify_attestation" = 1 ]; then
        have gh || fail "--verify-attestation needs the GitHub CLI (gh)"
        gh attestation verify "$tmp/$archive" --repo "$REPO_SLUG" >/dev/null \
            || fail "the build provenance of $archive could not be verified; nothing was installed"
        say "Build provenance ok"
    fi

    tar -xzf "$tmp/$archive" -C "$tmp" "$name/gdt" || fail "could not unpack $archive"
    [ -f "$tmp/$name/gdt" ] || fail "$archive has no gdt executable"

    mkdir -p "$dir" || fail "could not create $dir"
    # Copy beside the final name, then rename: a gdt that is running keeps working.
    staged="$dir/.gdt.new.$$"
    cp "$tmp/$name/gdt" "$staged" || fail "could not write to $dir"
    chmod 755 "$staged"
    mv -f "$staged" "$dir/gdt" || { rm -f "$staged"; fail "could not replace $dir/gdt"; }

    say "Installed $("$dir/gdt" --version 2>/dev/null || printf 'gdt') in $dir"
    case ":${PATH:-}:" in
        *":$dir:"*) ;;
        *)
            say ""
            say "$dir is not on your PATH. Add it, for example:"
            say "  export PATH=\"$dir:\$PATH\""
            say "(put that line in your shell profile to keep it). The installer changes nothing else."
            ;;
    esac
}

main "$@"
