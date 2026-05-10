# PRD: Directory Setting for HQ macOS App

## Problem

The HQ macOS app uses a hardcoded default directory (`~/git/hq`), overridable only via the `HQ_DIR` environment variable. To switch directories (e.g., real HQ vs. a sanitized demo) the user must quit the app, set `HQ_DIR` in a terminal, and relaunch from that terminal context.

## Goal

Let the user pick the HQ data directory from a Settings panel inside the app. That's it. No recent-directories list, no multi-window, no per-document model.

## Functional Requirements

### FR-1: Settings panel with directory picker
- **Where:** `HQ → Settings…` (⌘,)
- **UI:** Single field showing the current directory path + `[Choose…]` button.
- **Behavior:** `[Choose…]` opens `NSOpenPanel` restricted to directories. On selection, the path is saved and the server reloads against the new directory.
- **Persistence:** Stored in `UserDefaults` under key `HQDataDir`.

### FR-2: Directory resolution order
On launch, resolve the data directory in this order:
1. `HQ_DIR` environment variable (if set) — takes precedence, **not** written back to `UserDefaults`.
2. `UserDefaults` `HQDataDir` (if set and the path still exists).
3. Bundled `HQDataDir` from `Info.plist` (current fallback).
4. `~/git/hq`.

If the resolved directory doesn't exist, show the Settings panel with an inline error instead of failing silently.

### FR-3: Reload on change
When the user picks a new directory:
1. Terminate the current `hq serve` child process (only if we own it).
2. Start a new `hq serve --port <port>` against the new directory.
3. Reload the WebView once the server is reachable.

The existing "attach to running server on port" behavior in `ServerController` only applies on initial launch. Once the user explicitly chooses a directory, the app always owns its server.

### FR-4: Validation
A directory is valid if `hq` can load it — i.e., the same auto-discovery used by the CLI succeeds (see `src/config.rs`). On invalid selection, show an inline error in the Settings panel ("No HQ tracks found in /path/to/dir") and leave the previous directory active. Don't reload the server.

## Out of Scope

- Recent directories / Open Recent menu
- File → Open… and ⌘O
- Multiple windows
- Showing the directory path in the window title
- Creating a new HQ skeleton from the app
- Unsaved-edit prompts (the side-panel editor saves on blur; a switch will drop any in-flight edit — acceptable for now)

## Acceptance Criteria

| ID | Criteria |
|----|----------|
| AC-1 | Settings panel has a directory field and Choose… button that opens NSOpenPanel |
| AC-2 | Selecting a valid directory reloads the WebView with that directory's projects |
| AC-3 | Selected directory persists across app restarts |
| AC-4 | `HQ_DIR` env var overrides the saved setting without overwriting it |
| AC-5 | Invalid directory shows an inline error and leaves current directory active |
| AC-6 | Missing saved directory on launch opens Settings instead of crashing |

## Implementation Notes

All Swift code lives in `macos/HQDesktop/HQDesktopApp.swift` (single file).

- Extend `ServerController` with `reload(directory:)` that terminates `process`, clears `ownsServer`/`didStart`, updates `hqDir`, and re-invokes `start()`.
- Add a `Settings` scene to `HQDesktopApp` body alongside `WindowGroup`.
- Use `@AppStorage("HQDataDir")` in the Settings view, bound to the path field.
- For validation, shell out to `hq --dir <path> tracks` (or a dedicated `hq check` subcommand if we add one) and check exit status. Avoid duplicating the auto-discovery heuristic in Swift.

## Related Files

- `macos/HQDesktop/HQDesktopApp.swift` — app, `ServerController`, `ContentView`, `HQWebView`
- `src/config.rs` — canonical directory validation logic
- `script/build_and_run.sh` — build script (no changes expected)
