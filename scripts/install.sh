#!/bin/sh
# Build target/LVTeX.app and install it into /Applications, quitting a running copy first.
set -eu
cd "$(dirname "$0")/.."
root="$PWD"

sh "$root/scripts/build-app.sh"

if pgrep -xq LVTeX; then
    osascript -e 'quit app "LVTeX"'
    tries=0
    while pgrep -xq LVTeX; do
        tries=$((tries + 1))
        if [ "$tries" -ge 10 ]; then
            printf '%s\n' "error: LVTeX is still running; quit it and run again" >&2
            exit 1
        fi
        sleep 1
    done
fi

# Copy beside the old bundle first, then swap, so a failed copy leaves the old app in place.
rm -rf /Applications/LVTeX.app.next
ditto "$root/target/LVTeX.app" /Applications/LVTeX.app.next
rm -rf /Applications/LVTeX.app
mv /Applications/LVTeX.app.next /Applications/LVTeX.app

printf '%s\n' /Applications/LVTeX.app
