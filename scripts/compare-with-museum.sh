#!/usr/bin/env bash
# Renders skins of the Winamp Skin Museum by their md5 and measures each preview
# against the museum's own screenshot.
# Usage: scripts/compare-with-museum.sh <md5> [<md5> ...]
# Output goes to out/museum: <md5>.png (ours), <md5>.museum.png, <md5>.diff.png.
set -euo pipefail

cdn=https://r2.webampskins.org
base_md5=5e4f10275dcb1fb211d4a8b4f1bda236 # Winamp's base skin
tool=${WALKIT:-target/release/walkit}
out=out/museum

if [ $# -eq 0 ]; then
    echo "usage: $0 <md5> [<md5> ...]" >&2
    exit 2
fi
[ -x "$tool" ] || cargo build --release
mkdir -p "$out"

status=0
for md5 in "$@"; do
    echo "== $md5"
    if ! "$tool" render "$cdn/skins/$md5.wsz" --base "$cdn/skins/$base_md5.wsz" \
        --output "$out/$md5.png" --quiet; then
        status=1
        continue
    fi
    curl --silent --show-error --fail --output "$out/$md5.museum.png" \
        "$cdn/screenshots/$md5.png" || { status=1; continue; }
    "$tool" compare "$out/$md5.png" "$out/$md5.museum.png" --bands 116 \
        --diff "$out/$md5.diff.png" || status=1
done
exit $status
