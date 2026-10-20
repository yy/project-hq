import Foundation
import WebKit
import XCTest
@testable import HQDesktop

@MainActor
final class UpdateControllerTests: XCTestCase {
    private final class Navigation: NSObject, WKNavigationDelegate {
        let loaded: XCTestExpectation
        init(_ loaded: XCTestExpectation) { self.loaded = loaded }
        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) { loaded.fulfill() }
    }

    private func window(ready: Bool) async -> WKWebView {
        let view = WKWebView()
        let loaded = expectation(description: "Update test window loaded")
        let navigation = Navigation(loaded)
        view.navigationDelegate = navigation
        view.loadHTMLString("""
            <script>
            window.ready = \(ready);
            window.hqPrepareForUpdate = async () => { document.body.inert = window.ready; return window.ready; };
            window.hqCancelUpdate = () => { document.body.inert = false; };
            </script><body>Update bridge fixture</body>
            """, baseURL: nil)
        await fulfillment(of: [loaded], timeout: 10)
        withExtendedLifetime(navigation) {}
        return view
    }

    private var configuration: UpdateConfiguration {
        UpdateConfiguration(source: "/tmp/source", revision: String(repeating: "a", count: 40),
                            cache: URL(fileURLWithPath: "/tmp/hq-update-test"),
                            bundle: URL(fileURLWithPath: "/tmp/HQ.app"), resources: URL(fileURLWithPath: "/tmp/resources"))
    }

    private func defaults() -> UserDefaults {
        let name = "HQUpdates-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: name)!
        addTeardownBlock { defaults.removePersistentDomain(forName: name) }
        return defaults
    }

    private func eventually(_ condition: () -> Bool, file: StaticString = #filePath, line: UInt = #line) async {
        let deadline = ContinuousClock.now.advanced(by: .seconds(2))
        while !condition(), ContinuousClock.now < deadline { try? await Task.sleep(for: .milliseconds(5)) }
        XCTAssertTrue(condition(), file: file, line: line)
    }

    func testPendingBuildWaitsForWindowsAndServerWithoutRebuilding() async {
        var prepared = 0, installed = 0
        var windowsIdle = false, serverIdle = false
        let updater = UpdateController(configuration: configuration, defaults: defaults(), prepare: { _ in
            prepared += 1
            return .ready(String(repeating: "b", count: 40))
        }, prepareWindows: { windowsIdle }, install: { _ in installed += 1 })
        defer { updater.stop() }
        updater.verifyServerIdle = { serverIdle }
        updater.checkNow()
        await eventually { !updater.checking }
        XCTAssertEqual(installed, 0)
        windowsIdle = true
        updater.checkNow()
        await eventually { !updater.checking }
        XCTAssertEqual(installed, 0)
        serverIdle = true
        updater.checkNow()
        await eventually { installed == 1 }
        XCTAssertEqual(prepared, 1)
    }

    func testRealWebKitBridgeRequiresEveryWindowAndUnfreezesOnDeferral() async throws {
        let first = await window(ready: true)
        let second = await window(ready: false)
        var installed = 0
        let updater = UpdateController(configuration: configuration, defaults: defaults(), prepare: { _ in
            .ready(String(repeating: "b", count: 40))
        }, install: { _ in installed += 1 })
        defer { updater.stop() }
        updater.register(first)
        updater.register(second)
        updater.verifyServerIdle = { true }
        updater.checkNow()
        await eventually { !updater.checking }
        XCTAssertEqual(installed, 0)
        let unlocked = try await first.evaluateJavaScript("document.body.inert")
        XCTAssertEqual(unlocked as? Bool, false)
        _ = try await second.evaluateJavaScript("window.ready = true")
        updater.checkNow()
        await eventually { installed == 1 }
        let locked = try await first.evaluateJavaScript("document.body.inert")
        XCTAssertEqual(locked as? Bool, true)
    }

    func testDisablingDuringBuildPreventsRestartAndCheckDoesNotOverlap() async {
        var continuation: CheckedContinuation<UpdateResult, Never>?
        var prepared = 0, installed = 0
        let updater = UpdateController(configuration: configuration, defaults: defaults(), prepare: { _ in
            prepared += 1
            return await withCheckedContinuation { continuation = $0 }
        }, prepareWindows: { true }, install: { _ in installed += 1 })
        defer { updater.stop() }
        updater.verifyServerIdle = { true }
        updater.checkNow()
        await eventually { continuation != nil }
        updater.checkNow()
        updater.enabled = false
        continuation?.resume(returning: .ready(String(repeating: "b", count: 40)))
        await eventually { !updater.checking }
        XCTAssertEqual(prepared, 1)
        XCTAssertEqual(installed, 0)
        updater.enabled = true
        await eventually { installed == 1 }
        XCTAssertEqual(prepared, 1)
    }

    func testPeriodicChecksRespectIntervalAndOptOut() async throws {
        var checks = 0
        let updater = UpdateController(configuration: configuration, defaults: defaults(), checkInterval: 0.08, pollInterval: 0.01,
                                       prepare: { _ in checks += 1; return .current })
        defer { updater.stop() }
        updater.start()
        await eventually { checks >= 2 }
        updater.enabled = false
        let count = checks
        try await Task.sleep(for: .milliseconds(150))
        XCTAssertEqual(checks, count)
        updater.checkNow()
        await eventually { checks == count + 1 }
    }

    func testStoppingDuringCheckCannotInstallItsLateResult() async {
        var continuation: CheckedContinuation<UpdateResult, Never>?
        var installed = false
        let updater = UpdateController(configuration: configuration, defaults: defaults(), prepare: { _ in
            await withCheckedContinuation { continuation = $0 }
        }, prepareWindows: { true }, install: { _ in installed = true })
        updater.verifyServerIdle = { true }
        updater.checkNow()
        await eventually { continuation != nil }
        updater.stop()
        continuation?.resume(returning: .ready(String(repeating: "b", count: 40)))
        await eventually { !updater.checking }
        XCTAssertFalse(installed)
    }

    func testServerGateIncludesBackgroundAgentsAndOverviewGeneration() {
        let idle = Data("{\"overviews\":{}}".utf8)
        XCTAssertTrue(UpdateController.serverIsIdle(chats: Data("[]".utf8), projects: idle))
        for chat in ["{\"status\":\"running\"}", "{\"status\":\"applying\"}", "{\"status\":\"idle\",\"active_turn_id\":\"turn\"}", "{\"status\":\"idle\",\"ui\":{\"queue\":[{}]}}"] {
            XCTAssertFalse(UpdateController.serverIsIdle(chats: Data("[\(chat)]".utf8), projects: idle))
        }
        XCTAssertFalse(UpdateController.serverIsIdle(chats: Data("[]".utf8), projects: Data("{\"overviews\":{\"p\":{\"generating\":true}}}".utf8)))
        XCTAssertFalse(UpdateController.serverIsIdle(chats: Data("{}".utf8), projects: idle))
    }

    func testResultParserRejectsUnrecognizedOrInvalidBuildResults() throws {
        XCTAssertEqual(try UpdateResult(output: "compiler output\nHQ_UPDATE current\n"), .current)
        XCTAssertEqual(try UpdateResult(output: "HQ_UPDATE deferred Local edits.\n"), .deferred("Local edits."))
        XCTAssertThrowsError(try UpdateResult(output: "HQ_UPDATE ready unexpected/path"))
        XCTAssertThrowsError(try UpdateResult(output: "build failed"))
    }
}
