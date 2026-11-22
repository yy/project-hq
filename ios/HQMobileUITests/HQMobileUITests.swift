import XCTest

final class HQMobileUITests: XCTestCase {
    func testReviewAndCapture() {
        let app = XCUIApplication()
        app.launch()
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Neighborhood walk")).firstMatch.tap()
        XCTAssertTrue(app.staticTexts["Overview"].exists)
        app.buttons["Full notes"].tap()
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "The library offered its courtyard")).firstMatch.exists)
        app.navigationBars.buttons.firstMatch.tap()
        app.buttons["Quick capture"].tap()
        let field = app.textFields["What needs doing?"]
        field.tap()
        field.typeText("Check the garden entrance")
        app.buttons["Save"].tap()
        XCTAssertTrue(app.staticTexts["Task saved to Todos"].waitForExistence(timeout: 5))
    }
}
