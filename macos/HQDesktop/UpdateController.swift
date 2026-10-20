import AppKit
import Combine
import Foundation
import SwiftUI
import WebKit

struct UpdateConfiguration {
    let source: String
    let revision: String
    let cache: URL
    let bundle: URL
    let resources: URL

    static func bundled() -> Self? {
        let bundle = Bundle.main
        guard bundle.bundleURL.path == "/Applications/HQ.app",
              let source = bundle.object(forInfoDictionaryKey: "HQSourceDirectory") as? String,
              let revision = bundle.object(forInfoDictionaryKey: "HQRevision") as? String,
              let resources = bundle.resourceURL,
              !source.isEmpty, !revision.isEmpty else { return nil }
        return Self(source: source, revision: revision,
                    cache: defaultCache,
                    bundle: bundle.bundleURL, resources: resources)
    }

    static var defaultCache: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("net.yulab.project-hq/updates")
    }

    var stagedApp: URL { cache.appendingPathComponent("source/dist/HQ.app") }
}

enum UpdateResult: Equatable {
    case current
    case deferred(String)
    case ready(String)

    init(output: String) throws {
        guard let line = output.components(separatedBy: .newlines).last(where: { $0.hasPrefix("HQ_UPDATE ") }) else {
            throw UpdateError.failed("Update check failed. See the update log in Settings.")
        }
        let value = String(line.dropFirst("HQ_UPDATE ".count))
        if value == "current" { self = .current }
        else if value.hasPrefix("deferred ") { self = .deferred(String(value.dropFirst(9))) }
        else if value.hasPrefix("ready ") {
            let revision = String(value.dropFirst(6))
            guard revision.count == 40, revision.allSatisfy(\.isHexDigit) else {
                throw UpdateError.failed("Invalid update revision.")
            }
            self = .ready(revision)
        } else { throw UpdateError.failed("Invalid update result.") }
    }
}

enum UpdateError: LocalizedError {
    case failed(String)
    var errorDescription: String? { if case .failed(let message) = self { return message }; return nil }
}

@MainActor
final class UpdateController: ObservableObject {
    static let enabledKey = "HQAutomaticUpdates"
    @Published var enabled: Bool {
        didSet {
            defaults.set(enabled, forKey: Self.enabledKey)
            if enabled { checkNow() }
        }
    }
    @Published private(set) var status = "Checks for pushed changes every 15 minutes."
    @Published private(set) var checking = false
    @Published private(set) var lastChecked: Date?

    let configuration: UpdateConfiguration?
    var supported: Bool { configuration != nil }
    var settingsOpen = false
    var verifyServerIdle: () async -> Bool = { false }
    private let defaults: UserDefaults
    private let checkInterval: TimeInterval
    private let pollInterval: TimeInterval
    private let prepare: ((UpdateConfiguration) async throws -> UpdateResult)?
    private let install: ((UpdateConfiguration) throws -> Void)?
    private let prepareWindows: (() async -> Bool)?
    private let acknowledgeExternalUpdate: ((UUID) throws -> Void)?
    private let terminate: @MainActor () -> Void
    private let windows = NSHashTable<WKWebView>.weakObjects()
    private var timer: Task<Void, Never>?
    private var operation: Task<Void, Never>?
    private var process: Process?
    private var nextCheck = Date.distantPast
    private var pendingRevision: String?
    private var stopped = false
    private var windowGeneration = 0

    init(configuration: UpdateConfiguration? = .bundled(), defaults: UserDefaults = .standard,
         checkInterval: TimeInterval = 15 * 60, pollInterval: TimeInterval = 30,
         prepare: ((UpdateConfiguration) async throws -> UpdateResult)? = nil,
         prepareWindows: (() async -> Bool)? = nil,
         install: ((UpdateConfiguration) throws -> Void)? = nil,
         acknowledgeExternalUpdate: ((UUID) throws -> Void)? = nil,
         terminate: @escaping @MainActor () -> Void = { NSApp.terminate(nil) }) {
        self.configuration = configuration
        self.defaults = defaults
        self.checkInterval = checkInterval
        self.pollInterval = pollInterval
        self.prepare = prepare
        self.prepareWindows = prepareWindows
        self.install = install
        self.acknowledgeExternalUpdate = acknowledgeExternalUpdate
        self.terminate = terminate
        enabled = defaults.object(forKey: Self.enabledKey) as? Bool ?? true
        if configuration == nil { status = "Automatic updates are available in installed development builds." }
    }

    func register(_ webView: WKWebView) { windows.add(webView); windowGeneration += 1 }

    func start() {
        guard supported, timer == nil else { return }
        stopped = false
        timer = Task { [weak self] in
            while !Task.isCancelled {
                guard let delay = self?.pollInterval else { return }
                do { try await Task.sleep(for: .seconds(delay)) } catch { return }
                guard let self, !self.stopped else { return }
                if self.enabled && (self.pendingRevision != nil || Date() >= self.nextCheck) { self.checkNow() }
            }
        }
    }

    func stop() {
        stopped = true
        timer?.cancel()
        timer = nil
        operation?.cancel()
        process?.terminate()
    }

    func checkNow() {
        guard let configuration, !checking, !stopped else { return }
        checking = true
        operation = Task {
            defer { checking = false; operation = nil }
            do {
                if pendingRevision == nil {
                    status = "Checking for updates; new versions build in the background…"
                    let result: UpdateResult
                    if let prepare { result = try await prepare(configuration) }
                    else {
                        let output = try await runScript("prepare_update.sh", arguments: [
                            configuration.source, configuration.revision, configuration.cache.path,
                            ProcessInfo.processInfo.environment["HQ_DIR"] ?? UserDefaults.standard.string(forKey: ServerController.dataDirDefaultsKey)
                                ?? Bundle.main.object(forInfoDictionaryKey: "HQDataDir") as? String ?? "~/git/hq",
                        ])
                        result = try UpdateResult(output: output)
                    }
                    guard !stopped, !Task.isCancelled else { return }
                    lastChecked = Date()
                    nextCheck = Date().addingTimeInterval(checkInterval)
                    switch result {
                    case .current: status = "HQ is up to date."
                    case .deferred(let reason): status = reason
                    case .ready(let revision): pendingRevision = revision
                    }
                }
                if enabled, pendingRevision != nil { try await installWhenIdle(configuration) }
                else if pendingRevision != nil { status = "Update ready. Enable automatic updates to install and restart when idle." }
            } catch {
                if !stopped { status = error.localizedDescription }
                nextCheck = Date().addingTimeInterval(checkInterval)
            }
        }
    }

    static func externalUpdateRequestID(_ url: URL) -> UUID? {
        guard let parts = URLComponents(url: url, resolvingAgainstBaseURL: false),
              parts.scheme == "hq", parts.host == "prepare-update", parts.path.isEmpty || parts.path == "/",
              parts.user == nil, parts.password == nil, parts.port == nil, parts.fragment == nil,
              let items = parts.queryItems, items.count == 1, items[0].name == "request",
              let value = items[0].value else { return nil }
        return UUID(uuidString: value)
    }

    func prepareForExternalUpdate(requestID: UUID) {
        guard !checking, !stopped else { return }
        checking = true
        operation = Task {
            defer { checking = false; operation = nil }
            status = "Command Center requested an update; checking for idle chats and saved edits."
            guard await prepareIdleWindows() != nil else {
                status = "HQ is busy. Command Center left it open; retry the update when idle."
                return
            }
            var quitting = false
            defer { if !quitting { unlockWindows() } }
            do {
                if let acknowledgeExternalUpdate { try acknowledgeExternalUpdate(requestID) }
                else {
                    let cache = configuration?.cache ?? UpdateConfiguration.defaultCache
                    let pending = cache.appendingPathComponent("external-\(requestID.uuidString).pending")
                    let ready = cache.appendingPathComponent("external-\(requestID.uuidString).ready")
                    // Moving the request is an atomic claim. If the caller timed out
                    // and moved it away first, a late server response cannot quit HQ.
                    guard try String(contentsOf: pending) == String(ProcessInfo.processInfo.processIdentifier) else {
                        throw UpdateError.failed("Update request does not match this HQ process.")
                    }
                    try FileManager.default.moveItem(at: pending, to: ready)
                }
                status = "Closing HQ for the Command Center update…"
                quitting = true
                terminate()
            } catch {
                status = "Command Center update was cancelled; HQ is still open."
            }
        }
    }

    private func canFinishRestart(_ generation: Int) -> Bool {
        !stopped && !settingsOpen && !Task.isCancelled && windowGeneration == generation && NSApp?.modalWindow == nil
    }

    private func prepareIdleWindows() async -> Int? {
        let windowGeneration = windowGeneration
        guard canFinishRestart(windowGeneration) else { return nil }
        let ready: Bool
        if let prepareWindows { ready = await prepareWindows() }
        else {
            guard !windows.allObjects.isEmpty else { return nil }
            ready = await lockWindows()
        }
        guard ready, await verifyServerIdle(), canFinishRestart(windowGeneration) else {
            unlockWindows()
            return nil
        }
        return windowGeneration
    }

    private func installWhenIdle(_ configuration: UpdateConfiguration) async throws {
        status = "Update ready; waiting for idle chats and saved edits before restarting."
        guard let windowGeneration = await prepareIdleWindows() else { return }
        var restarting = false
        defer { if !restarting { unlockWindows() } }
        guard enabled else { return }
        if install == nil {
            // Recheck source safety after a potentially long build/idle wait.
            let validation = try await runScript("prepare_update.sh", arguments: [
                configuration.source, pendingRevision!, configuration.cache.path, "", "--validate",
            ])
            let result = try UpdateResult(output: validation)
            guard result == .current else {
                pendingRevision = nil
                nextCheck = .distantPast
                if case .deferred(let reason) = result { status = reason }
                return
            }
            let staged = NSDictionary(contentsOf: configuration.stagedApp.appendingPathComponent("Contents/Info.plist"))
            guard staged?["HQRevision"] as? String == pendingRevision else {
                pendingRevision = nil
                nextCheck = .distantPast
                throw UpdateError.failed("The staged update changed. HQ will check again.")
            }
            let installed = NSDictionary(contentsOf: configuration.bundle.appendingPathComponent("Contents/Info.plist"))
            guard installed?["HQRevision"] as? String == configuration.revision else {
                pendingRevision = nil
                status = "HQ was rebuilt outside the updater. Relaunch to use the installed version."
                return
            }
        }
        guard enabled, canFinishRestart(windowGeneration) else { return }
        if let install { try install(configuration) }
        else { try await launchInstaller(configuration, windowGeneration: windowGeneration) }
        restarting = true
        status = "Restarting HQ to finish the update…"
        pendingRevision = nil
    }

    private func lockWindows() async -> Bool {
        for view in windows.allObjects {
            do {
                let result = try await view.callAsyncJavaScript(
                    "return await window.hqPrepareForUpdate?.() === true;", arguments: [:], in: nil, contentWorld: .page)
                guard result as? Bool == true else { unlockWindows(); return false }
            } catch { unlockWindows(); return false }
        }
        return true
    }

    private func unlockWindows() {
        for view in windows.allObjects { view.evaluateJavaScript("window.hqCancelUpdate?.()", completionHandler: nil) }
    }

    private func runScript(_ name: String, arguments: [String]) async throws -> String {
        guard let configuration else { throw UpdateError.failed("Updater unavailable.") }
        try FileManager.default.createDirectory(at: configuration.cache, withIntermediateDirectories: true)
        let log = configuration.cache.appendingPathComponent("update.log")
        FileManager.default.createFile(atPath: log.path, contents: nil)
        let handle = try FileHandle(forWritingTo: log)
        defer { try? handle.close() }
        let child = Process()
        child.executableURL = URL(fileURLWithPath: "/bin/bash")
        child.arguments = [configuration.resources.appendingPathComponent(name).path] + arguments
        child.standardOutput = handle
        child.standardError = handle
        process = child
        defer { process = nil }
        let timeout = Task {
            do { try await Task.sleep(for: .seconds(20 * 60)) } catch { return }
            if child.isRunning {
                child.terminate()
                try? await Task.sleep(for: .seconds(2))
                if child.isRunning { kill(child.processIdentifier, SIGKILL) }
            }
        }
        defer { timeout.cancel() }
        let code: Int32 = try await withCheckedThrowingContinuation { continuation in
            child.terminationHandler = { child in continuation.resume(returning: child.terminationStatus) }
            do { try child.run() } catch { continuation.resume(throwing: error) }
        }
        guard code == 0 else { throw UpdateError.failed("Update failed; HQ is still running. See the update log in Settings.") }
        return try String(contentsOf: log, encoding: .utf8)
    }

    private func launchInstaller(_ configuration: UpdateConfiguration, windowGeneration: Int) async throws {
        let helper = configuration.cache.appendingPathComponent("install_update.sh")
        let helperData = try Data(contentsOf: configuration.resources.appendingPathComponent("install_update.sh"))
        try helperData.write(to: helper, options: .atomic)
        let log = configuration.cache.appendingPathComponent("install.log")
        FileManager.default.createFile(atPath: log.path, contents: nil)
        let handle = try FileHandle(forWritingTo: log)
        defer { try? handle.close() }
        let child = Process()
        let ready = configuration.cache.appendingPathComponent("installer-ready-\(UUID().uuidString)")
        child.executableURL = URL(fileURLWithPath: "/bin/bash")
        child.arguments = [helper.path, String(ProcessInfo.processInfo.processIdentifier), configuration.stagedApp.path,
                           configuration.bundle.path, configuration.cache.appendingPathComponent("previous.app").path, ready.path]
        child.standardOutput = handle
        child.standardError = handle
        child.standardInput = FileHandle.nullDevice
        try child.run()
        let deadline = ContinuousClock.now.advanced(by: .seconds(10))
        while !FileManager.default.fileExists(atPath: ready.path), child.isRunning, ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(50))
        }
        guard FileManager.default.fileExists(atPath: ready.path), child.isRunning, enabled, !stopped,
              !settingsOpen, self.windowGeneration == windowGeneration, !Task.isCancelled else {
            if child.isRunning { child.terminate() }
            throw UpdateError.failed("Could not start the installer. HQ is still running; see install.log beside the update log.")
        }
        terminate()
    }

    static func serverIsIdle(chats: Data, projects: Data) -> Bool {
        struct Queued: Decodable {}
        struct UI: Decodable { let queue: [Queued] }
        struct Chat: Decodable { let status: String; let active_turn_id: String?; let ui: UI? }
        struct Overview: Decodable { let generating: Bool }
        struct Projects: Decodable { let overviews: [String: Overview] }
        guard let chats = try? JSONDecoder().decode([Chat].self, from: chats),
              let projects = try? JSONDecoder().decode(Projects.self, from: projects) else { return false }
        return chats.allSatisfy { !["running", "applying"].contains($0.status) && $0.active_turn_id == nil && ($0.ui?.queue.isEmpty ?? true) }
            && projects.overviews.values.allSatisfy { !$0.generating }
    }
}

struct UpdateSettingsView: View {
    @ObservedObject var updater: UpdateController

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Toggle("Automatically update and restart when idle", isOn: $updater.enabled)
                .disabled(!updater.supported)
            Text(updater.status).font(.caption).foregroundStyle(.secondary)
            HStack {
                Button("Check Now") { updater.checkNow() }.disabled(!updater.supported || updater.checking)
                if updater.checking { ProgressView().controlSize(.small) }
                if let date = updater.lastChecked { Text(date, style: .time).font(.caption).foregroundStyle(.secondary) }
                if let configuration = updater.configuration {
                    Button("Update Log") { NSWorkspace.shared.open(configuration.cache.appendingPathComponent("update.log")) }
                }
            }
        }
    }
}
