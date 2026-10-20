#!/bin/bash
# Launched by HQ after drafts are saved and every window is idle.
set -euo pipefail
OLD_PID="$1"
STAGED_APP="$2"
INSTALLED_APP="$3"
BACKUP="$4"
READY="$5"
RELAUNCH="${6:-true}"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
[[ "$OLD_PID" =~ ^[0-9]+$ && ( "$OLD_PID" == 0 || "$OLD_PID" -gt 1 ) ]]
[[ "$RELAUNCH" == true || "$RELAUNCH" == false ]]
[[ "$INSTALLED_APP" == /Applications/HQ.app ]]
if [[ -e "$INSTALLED_APP" ]]; then
  [[ -w "$INSTALLED_APP" && -w "$INSTALLED_APP/Contents" ]]
else
  [[ -w /Applications ]]
fi
reopen() { if [[ "$RELAUNCH" == true ]]; then /usr/bin/open "$INSTALLED_APP"; fi; }
running_hq() { /usr/bin/pgrep -f '/HQ[.]app/Contents/MacOS/HQ( |$)' || true; }
for pid in $(running_hq); do
  if [[ "$pid" != "$OLD_PID" ]]; then echo 'Another HQ instance is running; update deferred.' >&2; exit 75; fi
done
/usr/bin/codesign --verify --deep --strict "$STAGED_APP"
trap 'trash "$READY" 2>/dev/null || true' EXIT
touch "$READY"
# Never replace a bundle while its old process is still running.
if [[ "$OLD_PID" != 0 ]]; then
  for ((i=0; i<120; i++)); do
    if ! kill -0 "$OLD_PID" 2>/dev/null; then break; fi
    sleep 0.25
  done
  if kill -0 "$OLD_PID" 2>/dev/null; then exit 75; fi
fi
if [[ -n "$(running_hq)" ]]; then echo 'HQ reopened before installation; update deferred.' >&2; exit 75; fi

HAVE_BACKUP=false
if [[ -e "$INSTALLED_APP" ]]; then
  if [[ -e "$BACKUP" ]] && ! trash "$BACKUP"; then reopen; exit 1; fi
  /usr/bin/ditto "$INSTALLED_APP" "$BACKUP" || { reopen; exit 1; }
  HAVE_BACKUP=true
fi
restore() {
  [[ "$HAVE_BACKUP" == true ]] || return 0
  if [[ -e "$INSTALLED_APP/Contents" ]]; then trash "$INSTALLED_APP/Contents"; fi
  /usr/bin/ditto "$BACKUP" "$INSTALLED_APP"
  reopen
}
# /Applications may forbid unlinking HQ.app itself. Its Contents directory is
# writable; replace it completely so removed resources cannot break signatures.
if [[ -e "$INSTALLED_APP/Contents" ]] && ! trash "$INSTALLED_APP/Contents"; then reopen; exit 1; fi
if ! /usr/bin/ditto "$STAGED_APP" "$INSTALLED_APP" \
    || ! /usr/bin/codesign --verify --deep --strict "$INSTALLED_APP"; then
  restore
  exit 1
fi
reopen || { restore; exit 1; }
