#!/bin/bash
# Pair this Mac's HQ with the iPhone through one private iCloud Drive folder.
set -euo pipefail
cloud="$HOME/Library/Mobile Documents/com~apple~CloudDocs"
vault="$(cd "$HOME/git/hq" && pwd -P)"
parent="$(cd "${1:-$cloud}" && pwd -P)"
cloud="$(cd "$cloud" && pwd -P)"
case "$parent/" in "$cloud/"*) ;; *) echo "Choose a folder in your iCloud Drive." >&2; exit 1 ;; esac
folder="$parent/HQ Live"
if ! mkdir -m 700 "$folder"; then
    echo "HQ Live already exists. Its pairing and contents were preserved." >&2
    exit 1
fi
id="$(uuidgen)"
mkdir -m 700 "$folder/Captures"
printf '%s\n' "$id" > "$folder/hq-connection.txt"
defaults write net.yulab.project-hq HQMobileBridge -dict vault "$vault" folder "$folder" id "$id"
printf 'Paired %s with %s\n' "$vault" "$folder"
