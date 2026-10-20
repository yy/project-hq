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

    func testSuiteUpdateUsesIdleChecksEvenWhenAutomaticUpdatesAreDisabled() async {
        var idle = false, acknowledged = 0, terminated = 0
        let updater = UpdateController(configuration: configuration, defaults: defaults(), prepareWindows: { idle },
                                       acknowledgeExternalUpdate: { _ in acknowledged += 1 }, terminate: { terminated += 1 })
        defer { updater.stop() }
        updater.enabled = false
        updater.verifyServerIdle = { true }
        updater.prepareForExternalUpdate(requestID: UUID())
        await eventually { !updater.checking }
        XCTAssertEqual(acknowledged, 0)
        XCTAssertEqual(terminated, 0)
        idle = true
        updater.prepareForExternalUpdate(requestID: UUID())
        await eventually { terminated == 1 }
        XCTAssertEqual(acknowledged, 1)
    }

    func testSuiteUpdateCannotQuitAfterCallerCancelsRequest() async throws {
        let cache = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: cache, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: cache) }
        let config = UpdateConfiguration(source: configuration.source, revision: configuration.revision, cache: cache,
                                         bundle: configuration.bundle, resources: configuration.resources)
        var continuation: CheckedContinuation<Bool, Never>?
        var terminated = 0
        let updater = UpdateController(configuration: config, defaults: defaults(), prepareWindows: {
            await withCheckedContinuation { continuation = $0 }
        }, terminate: { terminated += 1 })
        defer { updater.stop() }
        updater.verifyServerIdle = { true }
        let request = UUID()
        let pending = cache.appendingPathComponent("external-\(request.uuidString).pending")
        try String(ProcessInfo.processInfo.processIdentifier).write(to: pending, atomically: true, encoding: .utf8)
        updater.prepareForExternalUpdate(requestID: request)
        await eventually { continuation != nil }
        try FileManager.default.moveItem(at: pending, to: cache.appendingPathComponent("cancelled"))
        continuation?.resume(returning: true)
        await eventually { !updater.checking }
        XCTAssertEqual(terminated, 0)
        XCTAssertFalse(FileManager.default.fileExists(atPath: cache.appendingPathComponent("external-\(request.uuidString).ready").path))
        XCTAssertTrue(updater.status.contains("cancelled"))
    }

    func testSuiteRequestAcknowledgesTheMatchingProcessBeforeQuitting() async throws {
        let cache = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: cache, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: cache) }
        let config = UpdateConfiguration(source: configuration.source, revision: configuration.revision, cache: cache,
                                         bundle: configuration.bundle, resources: configuration.resources)
        let request = UUID()
        let pending = cache.appendingPathComponent("external-\(request.uuidString).pending")
        let ready = cache.appendingPathComponent("external-\(request.uuidString).ready")
        let pid = String(ProcessInfo.processInfo.processIdentifier)
        try pid.write(to: pending, atomically: true, encoding: .utf8)
        var terminated = false
        let updater = UpdateController(configuration: config, defaults: defaults(), prepareWindows: { true }, terminate: {
            XCTAssertFalse(FileManager.default.fileExists(atPath: pending.path))
            XCTAssertEqual(try? String(contentsOf: ready), pid)
            terminated = true
        })
        defer { updater.stop() }
        updater.verifyServerIdle = { true }
        updater.prepareForExternalUpdate(requestID: request)
        await eventually { terminated }
    }

    func testUpdateURLRejectsArbitraryPathsAndMissingOrRepeatedRequests() {
        let request = UUID()
        XCTAssertEqual(UpdateController.externalUpdateRequestID(URL(string: "hq://prepare-update?request=\(request.uuidString)")!), request)
        for value in ["hq://prepare-update", "hq://prepare-update?request=bad", "hq://other?request=\(request)",
                      "https://prepare-update?request=\(request)", "hq://prepare-update/path?request=\(request)",
                      "hq://prepare-update?request=\(request)&request=\(request)"] {
            XCTAssertNil(UpdateController.externalUpdateRequestID(URL(string: value)!))
        }
    }
}
