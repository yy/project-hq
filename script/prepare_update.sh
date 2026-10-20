#!/bin/bash
# Build a pushed main revision without changing the developer's checkout.
set -euo pipefail
SOURCE="$1"
INSTALLED_REVISION="$2"
CACHE="$3"
export HQ_DIR="$4"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
export GIT_TERMINAL_PROMPT=0
export GIT_SSH_COMMAND='/usr/bin/ssh -oBatchMode=yes -oConnectTimeout=10'

defer() { echo "HQ_UPDATE deferred $*"; exit 0; }
git_source() { /usr/bin/git -C "$SOURCE" "$@"; }
clean_source() {
  [[ "$(git_source symbolic-ref --quiet --short HEAD)" == main ]] || return 1
  [[ -z "$(git_source status --porcelain --untracked-files=normal)" ]] || return 1
  [[ ! -d "$(git_source rev-parse --absolute-git-dir)/rebase-merge" ]] || return 1
  [[ ! -d "$(git_source rev-parse --absolute-git-dir)/rebase-apply" ]] || return 1
}
clean_source || defer 'Waiting for a clean main checkout.'
if [[ "${5:-}" == --validate ]]; then
  git_source merge-base --is-ancestor HEAD "$INSTALLED_REVISION" || defer 'Source advanced beyond this update.'
  echo 'HQ_UPDATE current'
  exit 0
fi
git_source -c http.lowSpeedLimit=1000 -c http.lowSpeedTime=20 fetch --no-tags origin main
REVISION="$(git_source rev-parse --verify 'FETCH_HEAD^{commit}')"
if [[ "$REVISION" == "$INSTALLED_REVISION" ]]; then
  echo 'HQ_UPDATE current'
  exit 0
fi
git_source merge-base --is-ancestor "$INSTALLED_REVISION" "$REVISION" || defer 'Upstream does not extend this app revision.'
git_source merge-base --is-ancestor HEAD "$REVISION" || defer 'Waiting for local commits to be pushed.'
clean_source || defer 'Source changed during the update check.'

mkdir -p "$CACHE"
LOCK="$CACHE/build-lock"
if [[ -f "$LOCK/pid" ]]; then
  OWNER="$(cat "$LOCK/pid")"
  if [[ "$OWNER" =~ ^[0-9]+$ ]] && ! kill -0 "$OWNER" 2>/dev/null; then trash "$LOCK"; fi
fi
mkdir "$LOCK" 2>/dev/null || defer 'Another update build is running.'
echo $$ > "$LOCK/pid"
BUILD_PID=''
cleanup() {
  if [[ -n "$BUILD_PID" ]]; then kill -TERM -- "-$BUILD_PID" 2>/dev/null || true; fi
  trash "$LOCK"
}
trap cleanup EXIT
trap 'exit 143' TERM
STAGING="$CACHE/source"
if [[ -e "$STAGING" ]]; then trash "$STAGING"; fi
mkdir -p "$STAGING"
git_source archive "$REVISION" | /usr/bin/tar -x -C "$STAGING"
export CARGO_TARGET_DIR="$CACHE/cargo"
export HQ_SWIFT_BUILD_DIR="$CACHE/swift"
export HQ_BUILD_SOURCE_DIR="$SOURCE"
export HQ_BUILD_REVISION="$REVISION"
# A separate job group lets cancellation stop Cargo/Swift and their children.
set -m
/bin/bash "$STAGING/script/build_and_run.sh" --build &
BUILD_PID=$!
wait "$BUILD_PID"
BUILD_PID=''
set +m
/usr/bin/codesign --verify --deep --strict "$STAGING/dist/HQ.app"
echo "HQ_UPDATE ready $REVISION"
