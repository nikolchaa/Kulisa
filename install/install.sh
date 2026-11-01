#!/bin/sh
# Installs the kulisa tool, on Linux (x86-64) and macOS:
#
#   curl -fsSL https://github.com/nikolchaa/Kulisa/releases/latest/download/install.sh | sh
#
# KULISA_HOME            where it goes (default ~/.kulisa)
# KULISA_RELEASES        where it downloads from (default the latest release)
# KULISA_NO_MODIFY_PATH  leave the shell's profile alone
set -eu

fail() {
    echo "error: $1" >&2
    exit 1
}

# In a function, so a download of this script cut short runs nothing.
main() {
    case "$(uname -s)" in
        Linux)
            os=linux
            [ "$(uname -m)" = x86_64 ] || fail "Kulisa has no download for Linux on $(uname -m) yet; build it from source"
            ;;
        Darwin) os=macos ;;
        *) fail "this script is for Linux and macOS; on Windows use install.ps1" ;;
    esac
    releases=${KULISA_RELEASES:-https://github.com/nikolchaa/Kulisa/releases/latest/download}
    home=${KULISA_HOME:-$HOME/.kulisa}
    archive=kulisa-$os.tar.gz
    if [ -e "$home" ] && [ ! -x "$home/kulisa" ]; then
        fail "$home is in the way and is not a kulisa install; set KULISA_HOME to another folder"
    fi

    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT
    echo "downloading $archive..."
    curl -fL --progress-bar -o "$work/$archive" "$releases/$archive"
    curl -fsSL -o "$work/SHA256SUMS" "$releases/SHA256SUMS"
    want=$(awk -v f="$archive" '$2 == f || $2 == "*" f { print $1 }' "$work/SHA256SUMS")
    if command -v sha256sum > /dev/null; then
        got=$(sha256sum "$work/$archive" | cut -d ' ' -f 1)
    else
        got=$(shasum -a 256 "$work/$archive" | cut -d ' ' -f 1)
    fi
    [ -n "$want" ] && [ "$got" = "$want" ] || fail "the checksum of $archive is wrong; try again"

    tar xzf "$work/$archive" -C "$work"
    rm -rf "$home"
    mkdir -p "$(dirname "$home")"
    mv "$work/kulisa" "$home"
    echo "installed in $home"

    case ":$PATH:" in
        *":$home:"*) return ;;
    esac
    line="export PATH=\"$home:\$PATH\""
    if [ -n "${KULISA_NO_MODIFY_PATH:-}" ]; then
        echo "to use it from anywhere, add this to your shell's profile: $line"
        return
    fi
    case "${SHELL:-}" in
        */zsh) profile=$HOME/.zshrc ;;
        */bash) profile=$HOME/.bashrc ;;
        *) profile=$HOME/.profile ;;
    esac
    grep -qsF "$line" "$profile" || printf '\n%s\n' "$line" >> "$profile"
    echo "added it to the PATH in $profile; open a new terminal and run: kulisa help"
}

main
