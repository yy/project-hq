# Directory Selection and First-Run Setup

Status: implemented.

## Current behavior

The macOS app can switch HQ data directories from `HQ -> Settings`. The
Settings view shows the active path and a `Choose...` button. The picker accepts
directories only, validates the selection with `hq check`, stores it in
`UserDefaults` under `HQDataDir`, and restarts the bundled server against the
new directory.

`HQ_DIR` remains an explicit launch override. While it is set, the Settings
view explains that the environment value takes precedence and does not replace
the saved directory.

## Directory resolution

The app resolves the data directory in this order:

1. `HQ_DIR`, when set.
2. `UserDefaults` value `HQDataDir`.
3. Bundled `HQDataDir` from `Info.plist`.
4. `~/git/hq`.

Development builds bundle `HQ_DIR` or `~/git/hq`. `script/build_and_run.sh
--dist` bundles `~/Documents/HQ`.

If the resolved directory does not exist, the app opens a first-run welcome
view. The user can:

- create and seed the default folder with `hq init`; or
- choose an existing valid HQ directory.

The app uses the environment path as the creation target when `HQ_DIR` names a
missing directory. Otherwise, it creates `~/Documents/HQ`.

## Server reload

Selecting a directory from Settings:

1. terminates the current child server when HQ owns it;
2. starts `hq --dir <path> serve --port 0` with a fresh authentication token;
3. reads the assigned port from the server's startup handshake; and
4. shows the WebView when the authenticated server responds.

On every launch and reload, HQ starts and owns a private loopback server. It
never attaches to an unrelated existing server. Standalone CLI servers remain
independent.

## Validation

The Swift wrapper runs:

```bash
hq --dir <path> check
```

The CLI uses the same track discovery and frontmatter rules as the server.
Invalid selections leave the current directory active and show the CLI error in
the Settings view.

## Current limits

- No recent-directory list or `File -> Open`.
- One app window and one active data directory.
- No directory path in the window title.
- No prompt for an unsaved project-body or metadata edit before switching.

## Implementation

- `macos/HQDesktop/HQDesktopApp.swift`: resolution, first-run view, Settings,
  validation, and server reload.
- `src/main.rs`: `check` and `init` commands.
- `src/commands.rs`: starter HQ content.
- `script/build_and_run.sh`: bundled defaults and distribution build.
