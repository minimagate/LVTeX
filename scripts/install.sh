#!/bin/sh
# Build target/Mathstral.app and install it into /Applications, quitting a running copy first.
set -eu
cd "$(dirname "$0")/.."
root="$PWD"

sh "$root/scripts/build-app.sh"

if pgrep -xq Mathstral; then
    osascript -e 'quit app "Mathstral"'
    tries=0
    while pgrep -xq Mathstral; do
        tries=$((tries + 1))
        if [ "$tries" -ge 10 ]; then
            printf '%s\n' "error: Mathstral is still running; quit it and run again" >&2
            exit 1
        fi
        sleep 1
    done
fi

# Copy beside the old bundle first, then swap, so a failed copy leaves the old app in place.
rm -rf /Applications/Mathstral.app.next
ditto "$root/target/Mathstral.app" /Applications/Mathstral.app.next
rm -rf /Applications/Mathstral.app
mv /Applications/Mathstral.app.next /Applications/Mathstral.app

printf '%s\n' /Applications/Mathstral.app
