# HQ for iPhone

Open `HQMobile.xcodeproj` in Xcode and run the HQMobile scheme. Choose a development team under Signing & Capabilities for a physical iPhone.

HQ opens to **My Plate** with your selected projects, standalone todos overdue or due in the next seven days, and available routines. **Lists** opens Waiting, Todos, or All Projects. The gear opens Settings with the swipe guide and app version. The phone keeps the latest library for offline reading.

The blue **+** saves a capture on the phone. iCloud carries it to the Mac, where desktop HQ adds it to `_tasks/todo.txt`. A capture can wait while the Mac sleeps or HQ is closed. Pull to refresh, or leave the app open for its next refresh.

## Quick actions

Swipe a project, todo, or routine all the way right to defer it until tomorrow. A shorter right swipe reveals **Defer** for choosing another date. Swipe left to choose a project status or move a todo between Available and Waiting. Tap the circle beside a todo or routine to complete it. On a routine, the ellipsis menu offers **Defer** and **Skip this occurrence**. Swiping left also reveals Skip. Edits appear on the phone immediately and sync when desktop HQ is running. If the item changed independently, the home screen shows which edit needs review.

## Connect once

Run `./script/connect_mobile.sh` on the Mac that hosts `~/git/hq`. It creates **HQ Live** in iCloud Drive. To reuse an existing iPhone Files permission, pass the path of the previously authorized iCloud folder as its parent. Keep the Mac app running. A fresh iPhone installation asks for access to **HQ Live** once. It remembers the connection across launches.

The Mac publishes configured project records, saved overviews, open todos, and routine schedules every 20 seconds when their contents change. Each phone capture has its own ID. The Mac records delivery before acknowledging the iCloud item. If a crash interrupts delivery and the todo file changes independently, HQ retains the pending capture for inspection. To stop the bridge on this Mac, run `defaults delete net.yulab.project-hq HQMobileBridge`.

UI tests use the bundled sample for review and capture, and an isolated library for swipe edits.
