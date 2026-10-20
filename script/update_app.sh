#!/bin/bash
# Command Center installation: ask HQ to save and quit, never force it closed.
set -euo pipefail

running_hq_pids() { /usr/bin/pgrep -f '/HQ[.]app/Contents/MacOS/HQ( |$)' || true; }
hq_app_path() {
    local binary
    binary=$(/bin/ps -p "$1" -o comm=)
    echo "${binary%/Contents/MacOS/HQ}"
}
request_hq_update() { /usr/bin/open -g -a "$1" "$2"; }

prepare_hq_update() (
    # The subshell owns these variables; keep them alive for its EXIT trap.
    cache="$1"
    attempts="${2:-40}"
    pids=$(running_hq_pids)
    if [[ -z "$pids" ]]; then echo 0; exit 0; fi
    if [[ "$pids" == *$'\n'* ]]; then
        echo 'Several HQ instances are open; close the extras before updating.' >&2
        exit 75
    fi
    pid="$pids"
    request=$(/usr/bin/uuidgen)
    mkdir -p "$cache"
    pending="$cache/external-$request.pending"
    ready="$cache/external-$request.ready"
    cancelled="$cache/external-$request.cancelled"
    trap 'for file in "$pending" "$ready" "$cancelled"; do if [[ -e "$file" ]]; then trash "$file" || true; fi; done' EXIT
    printf '%s' "$pid" > "$pending"
    if ! request_hq_update "$(hq_app_path "$pid")" "hq://prepare-update?request=$request"; then
        if [[ -e "$pending" ]]; then mv -n "$pending" "$cancelled" 2>/dev/null || true; fi
        if [[ ! -e "$ready" ]]; then
            echo 'HQ could not receive the update request; leaving it open.' >&2
            exit 75
        fi
    fi
    for ((i=0; i<attempts; i++)); do
        [[ ! -e "$ready" ]] || break
        sleep 0.25
    done
    # Race the app's atomic pending -> ready claim. If cancellation wins, the
    # app cannot acknowledge later and unexpectedly quit after we returned.
    if [[ ! -e "$ready" && -e "$pending" ]]; then mv -n "$pending" "$cancelled" 2>/dev/null || true; fi
    if [[ ! -e "$ready" || "$(cat "$ready")" != "$pid" ]]; then
        echo 'HQ is busy or needs a newer build. Leave it open and retry when idle.' >&2
        exit 75
    fi
    for ((i=0; i<attempts; i++)); do
        kill -0 "$pid" 2>/dev/null || break
        sleep 0.25
    done
    if [[ -n "$(running_hq_pids)" ]]; then
        echo 'HQ has not finished closing; update deferred.' >&2
        exit 75
    fi
    echo "$pid"
)

update_hq_app() {
    local staged="$1" root cache old_pid relaunch=false ready
    root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    cache="$HOME/Library/Application Support/net.yulab.project-hq/updates"
    /usr/bin/codesign --verify --deep --strict "$staged"
    mkdir -p "$cache"
    old_pid=$(prepare_hq_update "$cache")
    [[ "$old_pid" == 0 ]] || relaunch=true
    ready="$cache/installer-ready-$(/usr/bin/uuidgen)"
    /bin/bash "$root/script/install_update.sh" "$old_pid" "$staged" /Applications/HQ.app "$cache/previous.app" "$ready" "$relaunch"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then update_hq_app "$1"; fi
