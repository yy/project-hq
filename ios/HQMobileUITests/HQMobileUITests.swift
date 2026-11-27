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
    func testPlateAndFullSwipes() throws {
        continueAfterFailure = false
        let id = UUID()
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(id.uuidString)
        let captures = root.appendingPathComponent("Captures")
        try FileManager.default.createDirectory(at: captures, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try id.uuidString.write(to: root.appendingPathComponent("hq-connection.txt"), atomically: true, encoding: .utf8)
        let library = """
        {"hq_mobile":1,"statuses":["my-plate","waiting"],"projects":[
            {"id":"a","revision":"a","title":"Plate project","status":"my-plate","notes":"","stale":false,"priority":1},
            {"id":"b","revision":"b","title":"Other project","status":"waiting","notes":"","stale":false,"priority":1}],
         "todos":[{"line":1,"revision":"first","title":"Call Sam","waiting":false,"due":"2001-01-01"},
                  {"line":2,"revision":"second","title":"Send notes","waiting":false,"due":"2001-01-02"},
                  {"line":3,"revision":"third","title":"Undated todo","waiting":false}],
         "routines":[{"id":"r","title":"Weekly review","available_on":"2001-01-01","next_due":"2001-01-02"},
                     {"id":"s","title":"Future routine","available_on":"2099-01-01","next_due":"2099-01-02"}]}
        """
        try library.write(to: root.appendingPathComponent("library.json"), atomically: true, encoding: .utf8)
        let bookmark = try root.bookmarkData(options: .minimalBookmark, includingResourceValuesForKeys: nil, relativeTo: nil)
        let app = XCUIApplication()
        app.launchArguments = ["-liveHQ", id.uuidString, "-workspace", "<" + bookmark.map { String(format: "%02x", $0) }.joined() + ">"]
        app.launch()
        defer { app.terminate() }
        let project = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Plate project")).firstMatch
        XCTAssertTrue(project.waitForExistence(timeout: 12))
        XCTAssertTrue(app.staticTexts["Weekly review"].exists)
        XCTAssertFalse(app.staticTexts["Future routine"].exists)
        XCTAssertFalse(app.staticTexts["Undated todo"].exists)
        XCTAssertFalse(app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Other project")).firstMatch.exists)

        func fullSwipe(_ element: XCUIElement, right: Bool) {
            let y = element.frame.midY / app.frame.height
            let start = app.coordinate(withNormalizedOffset: CGVector(dx: right ? 0.12 : 0.88, dy: y))
            let end = app.coordinate(withNormalizedOffset: CGVector(dx: right ? 0.94 : 0.06, dy: y))
            start.press(forDuration: 0.01, thenDragTo: end, withVelocity: .fast, thenHoldForDuration: 0.05)
        }
        project.swipeLeft()
        if app.buttons["Move"].exists { app.buttons["Move"].tap() }
        XCTAssertTrue(app.navigationBars["Move project"].waitForExistence(timeout: 5))
        app.buttons["Cancel"].tap()
        fullSwipe(project, right: true)
        fullSwipe(app.staticTexts["Call Sam"], right: true)
        fullSwipe(app.staticTexts["Send notes"], right: false)

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
            let deferred = edits.filter { $0["action"] as? String == "defer" && $0["until"] as? String == tomorrow }
            let moved = edits.contains { $0["revision"] as? String == "second" && $0["waiting"] as? Bool == true }
            return Set(deferred.compactMap { $0["revision"] as? String }) == ["a", "first"] && moved
        }
        expectation(for: queued, evaluatedWith: nil)
        waitForExpectations(timeout: 10)
    }
    #endif

    private func attachScreen(_ app: XCUIApplication, name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
