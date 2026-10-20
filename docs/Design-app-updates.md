# Automatic desktop updates

Installed development builds check `origin/main` after launch and every 15 minutes.
HQ builds new revisions in its Application Support cache, then saves chat drafts and restarts after at least one minute without interaction.
Settings provides an automatic-update toggle, Check Now, status, and the update log.

Updates wait for running or queued agents, project overview generation, network requests, open forms, project editors, and native Settings.
Every window must be idle; a failed draft save prevents restart.
The server also checks background conversations before shutdown.

Command Center uses the same idle checks for manual suite updates.
It builds first, then sends `hq://prepare-update?request=<uuid>` to the running app.
Update URLs reuse an existing dashboard window so retries cannot create fresh, non-idle windows.
HQ saves drafts and atomically acknowledges that request before quitting; a cancelled or expired request cannot close the app later.
Busy apps and older builds without this handshake are skipped without force-quitting.
The installer keeps a closed app closed and reopens one that was running.

The updater requires a clean local `main` checkout and a remote revision that extends both the installed revision and local HEAD.
It fetches and archives the remote commit without changing the checkout or HQ data.
Local edits, unpushed commits, and divergent histories pause updates.
Build failures leave the running app in place; failed replacement or signature verification restores the previous app.

This uses the existing local Rust and Swift toolchains and Git authentication.
Distribution archives have no source-checkout link and do not enable this updater; release-based distribution remains separate work.
