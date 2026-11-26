import XCTest

final class HQMobileUITests: XCTestCase {
    func testReviewAndCapture() {
        let app = XCUIApplication()
        app.launchArguments = ["--sample-workspace"]
        app.launch()
        let myPlate = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "My Plate")).firstMatch
        XCTAssertTrue(myPlate.waitForExistence(timeout: 8))
        attachScreen(app, name: "Home")
        myPlate.tap()
        XCTAssertTrue(app.navigationBars["My Plate"].exists)
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Neighborhood walk")).firstMatch.tap()
        XCTAssertTrue(app.staticTexts["Overview"].exists)
        attachScreen(app, name: "Project")
        app.buttons["Full notes"].tap()
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "The library offered its courtyard")).firstMatch.exists)
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.navigationBars["My Plate"].exists)
        app.navigationBars.buttons.firstMatch.tap()
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Waiting")).firstMatch.tap()
        XCTAssertTrue(app.navigationBars["Waiting"].exists)
        app.buttons["Quick capture"].tap()
        let field = app.descendants(matching: .any).matching(identifier: "capture-title").firstMatch
        field.tap()
        field.typeText("Check the garden entrance")
        app.buttons["Save"].tap()
        XCTAssertTrue(app.staticTexts["Task saved to Todos"].waitForExistence(timeout: 5))
    }

    private func attachScreen(_ app: XCUIApplication, name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
