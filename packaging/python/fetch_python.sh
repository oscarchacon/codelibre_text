#!/bin/sh
# Puts the Python that runs the plugins in <folder>, for the installers to carry: the editor works
# offline from the first start, with no Python installed. Run by the release builds (and CI);
# users never download it.
#   packaging/python/fetch_python.sh <folder> [<target triple>]
# The installers carry it next to the editor (`bundled_interpreter` in python_plugin_host.rs):
#   Windows <app>\python, macOS Contents/Resources/python, Linux /usr/lib/codelibre-text/python.
# For a development build: packaging/python/fetch_python.sh target/debug/python
#
# Python 3.13, a relocatable CPython from python-build-standalone, checked against the SHA-256
# pinned below: the Python of Sublime Text's current plugin host (build 4201 on), so Package
# Control installs its libraries (`Lib/python313`, compiled ones too) and packages are tested on
# it. Security fixes until October 2029; native builds for x64 and ARM64 on every platform.
# Moving to a newer Python: change `python` / `release` and the hashes (from that release's
# SHA256SUMS), and check `sublime.version()` in assets/python/sublime.py maps it to a build.
set -eu

dest=${1:?usage: fetch_python.sh <folder> [<target triple>]}
target=${2:-}

release=20261026
python=3.13.18

if [ -z "$target" ]; then
    case "$(uname -s)-$(uname -m)" in
        Darwin-arm64) target=aarch64-apple-darwin ;;
        Darwin-x86_64) target=x86_64-apple-darwin ;;
        Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
        Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
        MINGW*-x86_64 | MSYS*-x86_64 | CYGWIN*-x86_64) target=x86_64-pc-windows-msvc ;;
        MINGW*-aarch64 | MSYS*-aarch64 | MINGW*-arm64 | MSYS*-arm64) target=aarch64-pc-windows-msvc ;;
        *) echo "no Python build for $(uname -s) $(uname -m): pass a target triple" >&2; exit 1 ;;
    esac
fi

case "$target" in
    aarch64-apple-darwin) sha=8392b6ea51d94391befb2c31ebe75463f045d0b69dfcf5e1bb3234e1f5fa3405 ;;
    x86_64-apple-darwin) sha=7412c19d896c99431b74a18a5643e110449a7f9590fbab179c3e5ab27b9aa9e3 ;;
    x86_64-unknown-linux-gnu) sha=df15fe60d3ff55bf9dcf25824b5572756bb8816ff93865a273d3301d7066a8b7 ;;
    aarch64-unknown-linux-gnu) sha=578d9da1983463ffb0c8c98ab0da2f18177f44069328eb0af847860c1eb05c3e ;;
    x86_64-pc-windows-msvc) sha=7369f7a35f261222a6ac8b3a87c1c88eeebee8af945d85346202591a64038dd3 ;;
    aarch64-pc-windows-msvc) sha=bd3315abb8abb8ce5b19c2c2b709d318f983e0b8753d0830831df07c46474699 ;;
    *) echo "no Python build for $target" >&2; exit 1 ;;
esac

file="cpython-$python+$release-$target-install_only_stripped.tar.gz"
# Already there (make_app.sh and install.sh call this at every build): nothing to download.
if [ "$(cat "$dest/BUILD" 2> /dev/null || true)" = "$file" ]; then
    echo "Python $python ($target) already in $dest"
    exit 0
fi
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

curl -fsSL --retry 3 -o "$work/$file" \
    "https://github.com/astral-sh/python-build-standalone/releases/download/$release/$(printf '%s' "$file" | sed 's/+/%2B/')"
if command -v sha256sum > /dev/null 2>&1; then
    actual=$(sha256sum "$work/$file" | cut -d ' ' -f 1)
else
    actual=$(shasum -a 256 "$work/$file" | cut -d ' ' -f 1)
fi
if [ "$actual" != "$sha" ]; then
    echo "$file: SHA-256 is $actual, expected $sha" >&2
    exit 1
fi

mkdir -p "$work/unpacked"
tar -xzf "$work/$file" -C "$work/unpacked"
root="$work/unpacked/python"

# What plugins never use: the test suite, IDLE, Tk and the C headers.
for lib in "$root/lib/python3.13" "$root/Lib"; do
    [ -d "$lib" ] || continue
    rm -rf "$lib/test" "$lib/idlelib" "$lib/tkinter" "$lib/turtledemo" "$lib/lib2to3" \
        "$lib/ctypes/test" "$lib/unittest/test" "$lib/sqlite3/test"
done
rm -rf "$root/include" "$root/tcl" "$root/share"
printf '%s\n' "$file" > "$root/BUILD"

rm -rf "$dest"
mkdir -p "$(dirname "$dest")"
mv "$root" "$dest"
echo "Python $python ($target) in $dest"
