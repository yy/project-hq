#![cfg(target_os = "macos")]

use std::process::Command;

fn handshake(mode: &str) -> (std::process::Output, bool) {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new("/bin/bash")
        .args(["-c", r#"
source "$1"
cache="$2"
mode="$3"
running_hq_pids() {
    case "$mode" in
        closed) return ;;
        multiple) printf '424242\n424243\n' ;;
        *) [[ -e "$cache/closed" ]] || echo 424242 ;;
    esac
}
hq_app_path() { echo /fixture/HQ.app; }
request_hq_update() {
    local request="${2##*request=}"
    local pending="$cache/external-$request.pending"
    local ready="$cache/external-$request.ready"
    case "$mode" in
        ready) mv -n "$pending" "$ready"; touch "$cache/closed" ;;
        wrong) printf wrong > "$ready" ;;
        busy) : ;;
        late)
            (/bin/sleep 0.5; if mv -n "$pending" "$ready" 2>/dev/null; then touch "$cache/closed"; fi) &
            ;;
        *) echo 'unexpected request' >&2; return 1 ;;
    esac
}
prepare_hq_update "$cache" 1
"#, "suite-update-test"])
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/script/update_app.sh"))
        .arg(root.path())
        .arg(mode)
        .output()
        .unwrap();
    // Shell command capture waits for the simulated late caller to finish.
    let closed = root.path().join("closed").exists();
    assert!(
        std::fs::read_dir(root.path()).unwrap().all(|item| {
            !item
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("external-")
        }),
        "handshake receipts must be cleaned after success or deferral; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    (output, closed)
}

#[test]
fn suite_update_requires_a_matching_acknowledgement_before_installing() {
    let (ready, closed) = handshake("ready");
    assert!(
        ready.status.success(),
        "{}",
        String::from_utf8_lossy(&ready.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&ready.stdout).trim(), "424242");
    assert!(closed);
    let (wrong, _) = handshake("wrong");
    assert_eq!(wrong.status.code(), Some(75));
}

#[test]
fn busy_or_duplicate_apps_are_deferred_without_closing_them() {
    for mode in ["busy", "multiple", "late"] {
        let (result, closed) = handshake(mode);
        assert_eq!(
            result.status.code(),
            Some(75),
            "{mode}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            !closed,
            "{mode}: a cancelled request must not close the app later"
        );
    }
}

#[test]
fn closed_app_needs_no_quit_request_and_stays_closed() {
    let (result, closed) = handshake("closed");
    assert!(result.status.success());
    assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), "0");
    assert!(!closed);
}
