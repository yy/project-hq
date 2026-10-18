import Darwin
import Foundation
import XCTest
@testable import HQDesktop

@MainActor
final class ServerControllerTests: XCTestCase {
    private func fixture(_ body: String) throws -> (String, URL) {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let executable = directory.appendingPathComponent("hq-fixture")
        try ("#!/bin/sh\n" + body + "\n").write(to: executable, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: executable.path)
        addTeardownBlock { try FileManager.default.removeItem(at: directory) }
        return (directory.path, executable)
    }

    private var timing: ServerTiming {
        ServerTiming(startupTimeout: 1, startupPollInterval: 0.01, healthCheckInterval: 0.05,
                     requestTimeout: 0.1, retryDelay: 0.2, failureThreshold: 3)
    }

    private func eventually(_ condition: () -> Bool, file: StaticString = #filePath, line: UInt = #line) async {
        let deadline = ContinuousClock.now.advanced(by: .seconds(4))
        while !condition(), ContinuousClock.now < deadline {
            try? await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertTrue(condition(), file: file, line: line)
    }

    func testLateReadinessClearsStartupErrorWithoutRelaunch() async throws {
        let (directory, executable) = try fixture("echo 'HQ_READY {\"port\":34567}'; exec /bin/sleep 60")
        var responding = false
        let controller = ServerController(directory: directory, executableURL: executable, timing: timing) { request in
            XCTAssertEqual(request.url?.path, "/api/health")
            XCTAssertNil(request.url?.query)
            XCTAssertEqual(request.cachePolicy, .reloadIgnoringLocalCacheData)
            XCTAssertFalse(request.value(forHTTPHeaderField: "X-HQ-Token")?.isEmpty ?? true)
            return responding
        }
        defer { controller.stop() }
        controller.start()
        await eventually { controller.appURL != nil }
        let originalURL = controller.appURL
        await eventually { controller.canRetry }
        XCTAssertEqual(controller.status, "HQ server did not become ready")
        responding = true
        await eventually { controller.isReady }
        XCTAssertEqual(controller.appURL, originalURL)
        XCTAssertFalse(controller.canRetry)
        XCTAssertEqual(controller.detail, "")
    }

    func testStartupExitRetainsDiagnosticAndRetries() async throws {
        let (directory, executable) = try fixture("""
            if [ ! -f "$2/started" ]; then
                touch "$2/started"
                echo 'fixture startup failure' >&2
                exit 17
            fi
            echo 'HQ_READY {"port":34567}'
            exec /bin/sleep 60
            """)
        let controller = ServerController(directory: directory, executableURL: executable, timing: timing) { _ in true }
        defer { controller.stop() }
        controller.start()
        await eventually { controller.canRetry }
        XCTAssertEqual(controller.detail, "fixture startup failure")
        await eventually { controller.isReady }
    }

    func testFailedHealthChecksRestartAndRecover() async throws {
        let (directory, executable) = try fixture("echo 'HQ_READY {\"port\":34567}'; exec /bin/sleep 60")
        var responding = true
        let controller = ServerController(directory: directory, executableURL: executable, timing: timing) { _ in responding }
        defer { controller.stop() }
        controller.start()
        await eventually { controller.isReady }
        let firstURL = controller.appURL
        responding = false
        await eventually { controller.canRetry }
        responding = true
        await eventually { controller.isReady }
        XCTAssertNotEqual(controller.appURL, firstURL)
    }

    func testChildExitAfterReadinessRecovers() async throws {
        let (directory, executable) = try fixture("echo $$ > \"$2/pid\"; echo 'HQ_READY {\"port\":34567}'; exec /bin/sleep 60")
        let controller = ServerController(directory: directory, executableURL: executable, timing: timing) { _ in true }
        defer { controller.stop() }
        controller.start()
        await eventually { controller.isReady }
        let firstURL = controller.appURL
        let pid = try String(contentsOfFile: directory + "/pid").trimmingCharacters(in: .whitespacesAndNewlines)
        XCTAssertEqual(kill(try XCTUnwrap(Int32(pid)), SIGKILL), 0)
        await eventually { controller.canRetry }
        await eventually { controller.isReady }
        XCTAssertNotEqual(controller.appURL, firstURL)
    }

    func testReloadDoesNotBlockOnStoppedChild() async throws {
        let (directory, executable) = try fixture("echo $$ > \"$2/pid\"; echo 'HQ_READY {\"port\":34567}'; exec /bin/sleep 60")
        let controller = ServerController(directory: directory, executableURL: executable, timing: timing) { _ in true }
        defer { controller.stop() }
        controller.start()
        await eventually { controller.isReady }
        let pidText = try String(contentsOfFile: directory + "/pid").trimmingCharacters(in: .whitespacesAndNewlines)
        let pid = try XCTUnwrap(Int32(pidText))
        XCTAssertEqual(kill(pid, SIGSTOP), 0)
        controller.reload(directory: directory)
        await eventually { controller.isReady }
        await eventually { kill(pid, 0) != 0 }
    }

    func testReloadIgnoresAHealthResultFromTheOldServer() async throws {
        let (directory, executable) = try fixture("echo 'HQ_READY {\"port\":34567}'; exec /bin/sleep 60")
        var oldProbe: CheckedContinuation<Bool, Never>?
        var requests = 0
        let controller = ServerController(directory: directory, executableURL: executable, timing: timing) { _ in
            requests += 1
            if requests == 1 {
                return await withCheckedContinuation { oldProbe = $0 }
            }
            return false
        }
        defer { controller.stop() }
        controller.start()
        await eventually { oldProbe != nil }
        controller.reload(directory: directory)
        oldProbe?.resume(returning: true)
        await eventually { requests > 1 }
        XCTAssertFalse(controller.isReady)
        controller.stop()
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertNil(controller.appURL)
        XCTAssertFalse(controller.isReady)
    }

    func testFragmentedHandshakeAndInvalidPort() async throws {
        let (directory, executable) = try fixture("""
            echo 'HQ_READY {"port":99999}'
            printf 'HQ_RE'
            /bin/sleep 0.03
            printf 'ADY {"port":34567}\\n'
            exec /bin/sleep 60
            """)
        let controller = ServerController(directory: directory, executableURL: executable, timing: timing) { request in
            XCTAssertEqual(request.url?.port, 34567)
            return true
        }
        defer { controller.stop() }
        controller.start()
        await eventually { controller.isReady }
    }

    func testTransientTimeoutDoesNotReplaceReadyServer() async throws {
        let (directory, executable) = try fixture("echo 'HQ_READY {\"port\":34567}'; exec /bin/sleep 60")
        var requests = 0
        let controller = ServerController(directory: directory, executableURL: executable, timing: timing) { _ in
            requests += 1
            return requests != 2
        }
        defer { controller.stop() }
        controller.start()
        await eventually { controller.isReady }
        let firstURL = controller.appURL
        await eventually { requests >= 4 }
        XCTAssertTrue(controller.isReady)
        XCTAssertEqual(controller.appURL, firstURL)
    }
}
