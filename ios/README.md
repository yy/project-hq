# HQ for iPhone

Open `HQMobile.xcodeproj` in Xcode and run the HQMobile scheme. Choose a development team under Signing & Capabilities for a physical iPhone.

HQ opens straight into the projects and Todos in your HQ vault. Tap **My Plate** or **Waiting** to filter projects. Tap a project for the recorded next action, saved AI overview, and full notes. The phone keeps the latest library for offline reading.

The blue **+** saves a capture on the phone. iCloud carries it to the Mac, where desktop HQ adds it to `_tasks/todo.txt`. A capture can wait while the Mac sleeps or HQ is closed. Pull to refresh, or leave the app open for its next refresh.

## Quick actions

Swipe left on a project to move it or defer it. The same controls are in the project’s menu. Swipe left on a todo to defer it or switch between Available and Waiting. Tap its circle to complete it. Edits appear on the phone immediately and sync when desktop HQ is running. If the item changed independently, the home screen shows which edit needs review.

## Connect once

Run `./script/connect_mobile.sh` on the Mac that hosts `~/git/hq`. It creates **HQ Live** in iCloud Drive. To reuse an existing iPhone Files permission, pass the path of the previously authorized iCloud folder as its parent. Keep the Mac app running. A fresh iPhone installation asks for access to **HQ Live** once. It remembers the connection across launches.

The Mac publishes configured project records, saved overviews and open todos every 20 seconds when their contents change. Each phone capture has its own ID. The Mac records delivery before acknowledging the iCloud item. If a crash interrupts delivery and the todo file changes independently, HQ retains the pending capture for inspection. To stop the bridge on this Mac, run `defaults delete net.yulab.project-hq HQMobileBridge`.

UI tests use the bundled sample with the launch argument `--sample-workspace`.
