#!/bin/bash
# Launched by HQ after drafts are saved and every window is idle.
set -euo pipefail
OLD_PID="$1"
STAGED_APP="$2"
INSTALLED_APP="$3"
BACKUP="$4"
READY="$5"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
[[ "$OLD_PID" =~ ^[0-9]+$ && "$OLD_PID" -gt 1 ]]
[[ "$INSTALLED_APP" == /Applications/HQ.app ]]
[[ -w "$INSTALLED_APP" && -w "$INSTALLED_APP/Contents" ]]
/usr/bin/codesign --verify --deep --strict "$STAGED_APP"
trap 'trash "$READY" 2>/dev/null || true' EXIT
touch "$READY"
# Never replace a bundle while its old process is still running.
for ((i=0; i<120; i++)); do
  if ! kill -0 "$OLD_PID" 2>/dev/null; then break; fi
  sleep 0.25
done
if kill -0 "$OLD_PID" 2>/dev/null; then exit 1; fi

if [[ -e "$BACKUP" ]] && ! trash "$BACKUP"; then /usr/bin/open "$INSTALLED_APP"; exit 1; fi
/usr/bin/ditto "$INSTALLED_APP" "$BACKUP" || { /usr/bin/open "$INSTALLED_APP"; exit 1; }
restore() {
  if [[ -e "$INSTALLED_APP/Contents" ]]; then trash "$INSTALLED_APP/Contents"; fi
  /usr/bin/ditto "$BACKUP" "$INSTALLED_APP"
  /usr/bin/open "$INSTALLED_APP"
}
# /Applications may forbid unlinking HQ.app itself. Its Contents directory is
# writable; replace it completely so removed resources cannot break signatures.
if ! trash "$INSTALLED_APP/Contents"; then /usr/bin/open "$INSTALLED_APP"; exit 1; fi
if ! /usr/bin/ditto "$STAGED_APP" "$INSTALLED_APP" \
    || ! /usr/bin/codesign --verify --deep --strict "$INSTALLED_APP"; then
  restore
  exit 1
fi
/usr/bin/open "$INSTALLED_APP" || { restore; exit 1; }
