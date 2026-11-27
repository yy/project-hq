import XCTest

final class HQMobileUITests: XCTestCase {
    func testReviewAndCapture() {
        let app = XCUIApplication()
        app.launchArguments = ["--sample-workspace"]
        app.launch()
        let home = app.navigationBars["My Plate"]
        XCTAssertTrue(home.waitForExistence(timeout: 8))
        attachScreen(app, name: "Home")
        home.buttons["Settings"].tap()
        XCTAssertTrue(app.navigationBars["Settings"].exists)
        app.buttons["Done"].tap()
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Neighborhood walk")).firstMatch.tap()
        XCTAssertTrue(app.staticTexts["Overview"].exists)
        attachScreen(app, name: "Project")
        app.buttons["Full notes"].tap()
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "The library offered its courtyard")).firstMatch.exists)
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.navigationBars["My Plate"].exists)
        home.buttons["Lists"].tap()
        app.buttons["Waiting"].tap()
        XCTAssertTrue(app.navigationBars["Waiting"].exists)
        app.buttons["Quick capture"].tap()
        let field = app.descendants(matching: .any).matching(identifier: "capture-title").firstMatch
        field.tap()
        field.typeText("Check the garden entrance")
        app.buttons["Save"].tap()
        XCTAssertTrue(app.staticTexts["Task saved to Todos"].waitForExistence(timeout: 5))
    }

    #if targetEnvironment(simulator)
    private func withLibrary(_ library: String, test: (XCUIApplication, URL) throws -> Void) throws {
        continueAfterFailure = false
        let id = UUID()
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(id.uuidString)
        let captures = root.appendingPathComponent("Captures")
        try FileManager.default.createDirectory(at: captures, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try id.uuidString.write(to: root.appendingPathComponent("hq-connection.txt"), atomically: true, encoding: .utf8)
        try library.write(to: root.appendingPathComponent("library.json"), atomically: true, encoding: .utf8)
        let bookmark = try root.bookmarkData(options: .minimalBookmark, includingResourceValuesForKeys: nil, relativeTo: nil)
        let app = XCUIApplication()
        app.launchArguments = ["-liveHQ", id.uuidString, "-workspace", "<" + bookmark.map { String(format: "%02x", $0) }.joined() + ">"]
        app.launch()
        defer { app.terminate() }
        try test(app, captures)
    }

    private func fullSwipe(_ element: XCUIElement, in app: XCUIApplication, right: Bool) {
        let y = element.frame.midY / app.frame.height
        let start = app.coordinate(withNormalizedOffset: CGVector(dx: right ? 0.12 : 0.88, dy: y))
        let end = app.coordinate(withNormalizedOffset: CGVector(dx: right ? 0.94 : 0.06, dy: y))
        start.press(forDuration: 0.01, thenDragTo: end, withVelocity: .fast, thenHoldForDuration: 0.05)
    }

    private func checkEdits(in captures: URL, matching: @escaping ([[String: Any]], String) -> Bool) throws {
        let day = DateFormatter()
        day.locale = Locale(identifier: "en_US_POSIX")
        day.dateFormat = "yyyy-MM-dd"
        let tomorrow = day.string(from: try XCTUnwrap(Calendar.current.date(byAdding: .day, value: 1, to: .now)))
        let queued = NSPredicate { _, _ in
            let files = (try? FileManager.default.contentsOfDirectory(at: captures, includingPropertiesForKeys: nil)) ?? []
            let edits = files.compactMap { file -> [String: Any]? in
                guard let data = try? Data(contentsOf: file) else { return nil }
                return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
            }
            return matching(edits, tomorrow)
        }
        expectation(for: queued, evaluatedWith: nil)
        waitForExpectations(timeout: 10)
    }

    func testPlateAndFullSwipes() throws {
        let library = """
        {"hq_mobile":1,"statuses":["my-plate","waiting"],"projects":[
            {"id":"a","revision":"a","title":"Plate project","status":"my-plate","notes":"","stale":false,"priority":1},
            {"id":"b","revision":"b","title":"Other project","status":"waiting","notes":"","stale":false,"priority":1}],
         "todos":[{"line":1,"revision":"first","title":"Call Sam","waiting":false,"due":"2001-01-01"},
                  {"line":2,"revision":"second","title":"Send notes","waiting":false,"due":"2001-01-02"},
                  {"line":3,"revision":"third","title":"Undated todo","waiting":false}],
         "routines":[{"id":"_routines/review.md","revision":"rev-r","title":"Weekly review","available_on":"2001-01-01","next_due":"2001-01-02"},
                     {"id":"s","title":"Future routine","available_on":"2099-01-01","next_due":"2099-01-02"}]}
        """
        try withLibrary(library) { app, captures in
            let project = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Plate project")).firstMatch
            XCTAssertTrue(project.waitForExistence(timeout: 12))
            XCTAssertTrue(app.staticTexts["Weekly review"].exists)
            XCTAssertFalse(app.staticTexts["Future routine"].exists)
            XCTAssertFalse(app.staticTexts["Undated todo"].exists)
            XCTAssertFalse(app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Other project")).firstMatch.exists)

            fullSwipe(project, in: app, right: false)
            if app.buttons["Move"].waitForExistence(timeout: 2) { app.buttons["Move"].tap() }
            XCTAssertTrue(app.navigationBars["Move project"].waitForExistence(timeout: 5), app.debugDescription)
            app.buttons["Cancel"].tap()
            fullSwipe(project, in: app, right: true)
            fullSwipe(app.staticTexts["Call Sam"], in: app, right: true)
            fullSwipe(app.staticTexts["Send notes"], in: app, right: false)

            try checkEdits(in: captures) { edits, tomorrow in
                let deferred = edits.filter { $0["action"] as? String == "defer" && $0["until"] as? String == tomorrow }
                let moved = edits.contains { $0["revision"] as? String == "second" && $0["waiting"] as? Bool == true }
                return Set(deferred.compactMap { $0["revision"] as? String }) == ["a", "first"] && moved
            }
        }
    }

    func testRoutineActions() throws {
        let library = """
        {"hq_mobile":1,"projects":[],"todos":[],"routines":[
          {"id":"_routines/review.md","revision":"rev-r","title":"Weekly review","available_on":"2001-01-01","next_due":"2001-01-02"},
          {"id":"_routines/skip.md","revision":"rev-s","title":"Check bulbs","available_on":"2001-01-01","next_due":"2001-01-02"},
          {"id":"_routines/defer.md","revision":"rev-d","title":"Water plants","available_on":"2001-01-01","next_due":"2001-01-02"}]}
        """
        try withLibrary(library) { app, captures in
            XCTAssertTrue(app.buttons["Complete Weekly review"].waitForExistence(timeout: 12))
            let lastMenu = app.buttons["Actions for Water plants"]
            XCTAssertLessThan(lastMenu.frame.maxY, app.buttons["Quick capture"].frame.minY)
            app.buttons["Complete Weekly review"].tap()
            XCTAssertFalse(app.staticTexts["1 pending"].exists)
            app.navigationBars["My Plate"].buttons["Settings"].tap()
            XCTAssertTrue(app.staticTexts["Pending edits, 1"].waitForExistence(timeout: 5))
            app.buttons["Done"].tap()
            app.buttons["Actions for Check bulbs"].tap()
            app.buttons["Skip this occurrence"].tap()
            fullSwipe(app.staticTexts["Water plants"], in: app, right: true)
            XCTAssertFalse(app.staticTexts["Weekly review"].exists)
            XCTAssertFalse(app.staticTexts["Check bulbs"].exists)
            XCTAssertFalse(app.staticTexts["Water plants"].exists)

            try checkEdits(in: captures) { edits, tomorrow in
                func hasRoutineEvent(_ action: String, _ file: String, _ revision: String) -> Bool {
                    edits.contains { edit in
                        guard let at = edit["at"] as? String, ISO8601DateFormatter().date(from: at) != nil else { return false }
                        return edit["action"] as? String == action && edit["routine"] as? String == file && edit["revision"] as? String == revision
                    }
                }
                return edits.contains { $0["action"] as? String == "defer" && $0["routine"] as? String == "_routines/defer.md" && $0["until"] as? String == tomorrow }
                    && hasRoutineEvent("complete_routine", "_routines/review.md", "rev-r")
                    && hasRoutineEvent("skip_routine", "_routines/skip.md", "rev-s")
            }
        }
    }

    func testCachedPlateRetriesSyncWhileAnEditReceiptIsDelayed() throws {
        let original = """
        {"hq_mobile":1,"projects":[],"todos":[{"line":1,"title":"Saved todo","waiting":false,"due":"2001-01-01"}],
         "routines":[{"id":"review","revision":"rev","title":"Weekly review","available_on":"2001-01-01","next_due":"2001-01-02"}]}
        """
        let updated = """
        {"hq_mobile":1,"projects":[],"todos":[{"line":1,"title":"New from Mac","waiting":false,"due":"2001-01-01"}],"routines":[]}
        """
        try withLibrary(original) { app, captures in
            let library = captures.deletingLastPathComponent().appendingPathComponent("library.json")
            let complete = app.buttons["Complete Weekly review"]
            XCTAssertTrue(complete.waitForExistence(timeout: 12))
            try FileManager.default.removeItem(at: library)
            complete.tap()
            try checkEdits(in: captures) { edits, _ in edits.contains { $0["action"] as? String == "complete_routine" } }
            let edit = try XCTUnwrap(FileManager.default.contentsOfDirectory(at: captures, includingPropertiesForKeys: nil).first)
            let receipt = captures.appendingPathComponent(edit.lastPathComponent.replacingOccurrences(of: "Edit-", with: "Result-"))
            try Data().write(to: receipt, options: .atomic)

            app.terminate()
            app.launch()
            XCTAssertTrue(app.staticTexts["Saved todo"].waitForExistence(timeout: 10))
            XCTAssertFalse(complete.exists)
            let settings = app.navigationBars["My Plate"].buttons["Settings"]
            settings.tap()
            let waiting = app.staticTexts["Waiting for HQ Live in iCloud Drive."]
            XCTAssertTrue(waiting.waitForExistence(timeout: 5))
            XCTAssertTrue(app.staticTexts["Pending edits, 1"].exists)
            app.buttons["Done"].tap()
            XCTAssertFalse(waiting.exists)
            XCTAssertFalse(app.buttons["Connect to HQ"].exists)
            XCTAssertFalse(app.staticTexts.matching(NSPredicate(format: "label CONTAINS[c] %@", "offline")).firstMatch.exists)

            try updated.write(to: library, atomically: true, encoding: .utf8)
            XCTAssertTrue(app.staticTexts["New from Mac"].waitForExistence(timeout: 50))
            settings.tap()
            XCTAssertTrue(app.staticTexts["Pending edits, 1"].exists)
            app.buttons["Done"].tap()

            try "{}".write(to: receipt, atomically: true, encoding: .utf8)
            XCUIDevice.shared.press(.home)
            app.activate()
            settings.tap()
            XCTAssertTrue(app.staticTexts["Pending edits, 0"].waitForExistence(timeout: 10))
            XCTAssertFalse(FileManager.default.fileExists(atPath: receipt.path))
        }
    }
    #endif

    private func attachScreen(_ app: XCUIApplication, name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
