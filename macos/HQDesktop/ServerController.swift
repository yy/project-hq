import Combine
import Darwin
import Foundation

struct ServerTiming {
    var startupTimeout: TimeInterval = 15
    var startupPollInterval: TimeInterval = 0.2
    var healthCheckInterval: TimeInterval = 5
    var requestTimeout: TimeInterval = 2
    var retryDelay: TimeInterval = 3
    var failureThreshold = 3
}

@MainActor
final class ServerController: ObservableObject {
    static let dataDirDefaultsKey = "HQDataDir"

    @Published private(set) var isReady = false
    @Published private(set) var needsSetup = false
    @Published private(set) var canRetry = false
    @Published private(set) var status = "Starting HQ..."
    @Published private(set) var detail = ""
    @Published private(set) var hqDir: String
    @Published private(set) var appURL: URL?

    let envOverrideActive: Bool

    private let executableURL: URL?
    private let timing: ServerTiming
    private let healthProbe: (URLRequest) async -> Bool
    private var process: Process?
    private var stdoutPipe: Pipe?
    private var stderrPipe: Pipe?
    private var didStart = false
    private var supervisor: Task<Void, Never>?
    private var generation = UUID()
    private var authToken = ""
    private var stdoutBuffer = Data()
    private var lastServerError = ""

    init(
        directory: String? = nil,
        executableURL: URL? = nil,
        timing: ServerTiming = ServerTiming(),
        healthProbe: @escaping (URLRequest) async -> Bool = ServerController.probeServer
    ) {
        let environment = ProcessInfo.processInfo.environment
        let envDir = environment["HQ_DIR"].flatMap { $0.isEmpty ? nil : $0 }
        envOverrideActive = envDir != nil
        let savedDir = UserDefaults.standard.string(forKey: Self.dataDirDefaultsKey)
        let bundledDir = Bundle.main.object(forInfoDictionaryKey: "HQDataDir") as? String
        hqDir = Self.expandHome(directory ?? envDir ?? savedDir ?? bundledDir ?? "~/git/hq")
        self.executableURL = executableURL
        self.timing = timing
        self.healthProbe = healthProbe
    }

    func start() {
        guard !didStart else { return }
        didStart = true
        guard FileManager.default.isDirectory(atPath: hqDir) else {
            status = "Welcome to HQ"
            detail = ""
            needsSetup = true
            return
        }
        reload(directory: hqDir)
    }

    func stop() {
        generation = UUID()
        supervisor?.cancel()
        supervisor = nil
        terminateServer()
        isReady = false
        canRetry = false
        didStart = false
    }

    /// Cancel all work for the old directory before starting its replacement.
    func reload(directory: String) {
        stop()
        didStart = true
        needsSetup = false
        hqDir = Self.expandHome(directory)
        status = "Starting local HQ server..."
        detail = ""
        let generation = generation
        supervisor = Task { [weak self] in
            await self?.supervise(generation: generation)
        }
    }

    private func isCurrent(_ generation: UUID) -> Bool {
        self.generation == generation && !Task.isCancelled
    }

    private func supervise(generation: UUID) async {
        while isCurrent(generation) {
            do {
                try launchServer()
                await monitorServer(generation: generation)
            } catch {
                showFailure("Could not start HQ", fallback: error.localizedDescription)
            }
            guard isCurrent(generation) else { return }
            terminateServer()
            do {
                try await Task.sleep(for: .seconds(timing.retryDelay))
            } catch { return }
        }
    }

    private func monitorServer(generation: UUID) async {
        guard let child = process else { return }
        let deadline = ContinuousClock.now.advanced(by: .seconds(timing.startupTimeout))
        var awaitingStartup = true
        var failures = 0

        while isCurrent(generation) {
            let healthy = await canReachServer()
            // A directory switch or Retry may have happened during the request.
            guard isCurrent(generation), process === child else { return }
            guard child.isRunning else {
                showFailure("HQ server exited", fallback: "Exit status \(child.terminationStatus). Retrying automatically.")
                return
            }

            if healthy {
                // Also recover a late startup after its timeout was displayed.
                if !isReady {
                    isReady = true
                    canRetry = false
                    status = "HQ is ready"
                    detail = ""
                }
                awaitingStartup = false
                failures = 0
            } else if awaitingStartup {
                if ContinuousClock.now >= deadline {
                    let reason = appURL == nil
                        ? "HQ has not reported its listening port yet. Retrying automatically."
                        : "HQ is not responding yet. Retrying automatically."
                    showFailure("HQ server did not become ready", fallback: reason)
                    awaitingStartup = false
                }
            } else {
                failures += 1
                if failures >= timing.failureThreshold {
                    showFailure("Restarting local HQ server...", fallback: "HQ stopped responding. Retrying automatically.")
                    return
                }
            }

            do {
                let interval = awaitingStartup ? timing.startupPollInterval : timing.healthCheckInterval
                try await Task.sleep(for: .seconds(interval))
            } catch { return }
        }
    }

    private func showFailure(_ status: String, fallback: String) {
        isReady = false
        canRetry = true
        self.status = status
        detail = lastServerError.isEmpty ? fallback : lastServerError
    }

    private func launchServer() throws {
        lastServerError = ""
        let executableURL = try executableURL ?? Self.hqExecutableURL()
        guard FileManager.default.isDirectory(atPath: hqDir) else {
            throw AppError.missingHQDirectory(hqDir)
        }

        let process = Process()
        let launchToken = UUID().uuidString
        authToken = launchToken
        appURL = nil
        stdoutBuffer = Data()
        process.executableURL = executableURL
        process.currentDirectoryURL = URL(fileURLWithPath: hqDir)
        process.arguments = ["--dir", hqDir, "serve", "--port", "0", "--auth-token", launchToken]

        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe
        self.stdoutPipe = stdoutPipe
        self.stderrPipe = stderrPipe
        stdoutPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            if data.isEmpty { handle.readabilityHandler = nil }
            Task { @MainActor in
                self?.captureStandardOutput(data, launchToken: launchToken)
            }
        }
        stderrPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            if data.isEmpty { handle.readabilityHandler = nil }
            Task { @MainActor in
                guard let self, self.authToken == launchToken, !data.isEmpty else { return }
                self.lastServerError = String(
                    (self.lastServerError + String(decoding: data, as: UTF8.self)).suffix(8192)
                ).trimmingCharacters(in: .whitespacesAndNewlines)
                if self.canRetry { self.detail = self.lastServerError }
            }
        }

        try process.run()
        self.process = process
    }

    /// Detach first so output from a retiring child cannot affect a new launch.
    private func terminateServer() {
        let child = process
        process = nil
        authToken = ""
        appURL = nil
        stdoutPipe?.fileHandleForReading.readabilityHandler = nil
        stderrPipe?.fileHandleForReading.readabilityHandler = nil
        stdoutPipe = nil
        stderrPipe = nil
        guard let child, child.isRunning else { return }
        child.terminate()
        // Never waitUntilExit on the main actor: a stuck child must not freeze HQ.
        Task.detached {
            try? await Task.sleep(for: .seconds(1))
            if child.isRunning { kill(child.processIdentifier, SIGKILL) }
        }
    }

    private func captureStandardOutput(_ data: Data, launchToken: String) {
        guard launchToken == authToken else { return }
        stdoutBuffer.append(data)
        // Decode complete lines, since a pipe can split even a UTF-8 character.
        while let newline = stdoutBuffer.firstIndex(of: 10) {
            let line = String(decoding: stdoutBuffer[..<newline], as: UTF8.self)
            stdoutBuffer.removeSubrange(...newline)
            let prefix = "HQ_READY "
            guard line.hasPrefix(prefix) else { continue }
            struct ReadyMessage: Decodable { let port: Int }
            let json = Data(line.dropFirst(prefix.count).utf8)
            guard let ready = try? JSONDecoder().decode(ReadyMessage.self, from: json),
                  (1...65535).contains(ready.port),
                  var components = URLComponents(string: "http://127.0.0.1:\(ready.port)/")
            else {
                lastServerError = "Invalid HQ startup handshake"
                continue
            }
            components.queryItems = [URLQueryItem(name: "hq_token", value: launchToken)]
            appURL = components.url
        }
    }

    private func canReachServer() async -> Bool {
        guard process?.isRunning == true, let appURL else { return false }
        var components = URLComponents(url: appURL, resolvingAgainstBaseURL: false)!
        components.path = "/api/health"
        components.query = nil
        var request = URLRequest(url: components.url!, cachePolicy: .reloadIgnoringLocalCacheData)
        request.timeoutInterval = timing.requestTimeout
        request.setValue(authToken, forHTTPHeaderField: "X-HQ-Token")
        return await healthProbe(request)
    }

    private nonisolated static let healthSession: URLSession = {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.urlCache = nil
        return URLSession(configuration: configuration)
    }()

    nonisolated static func probeServer(_ request: URLRequest) async -> Bool {
        do {
            let (_, response) = try await healthSession.data(for: request)
            return (response as? HTTPURLResponse)?.statusCode == 204
        } catch {
            return false
        }
    }

    /// Adopt a directory as the HQ data dir: persist the choice and (re)start the server.
    func completeSetup(directory: String) {
        let expanded = Self.expandHome(directory)
        if !envOverrideActive {
            UserDefaults.standard.set(expanded, forKey: Self.dataDirDefaultsKey)
        }
        needsSetup = false
        reload(directory: expanded)
    }

    /// Run `hq --dir <path> init` to create and seed a starter HQ directory.
    /// Returns nil on success, error message on failure.
    nonisolated func initialize(directory: String) -> String? {
        let expanded = Self.expandHome(directory)
        let executableURL: URL
        do {
            executableURL = try Self.hqExecutableURL()
        } catch {
            return error.localizedDescription
        }

        let process = Process()
        process.executableURL = executableURL
        process.arguments = ["--dir", expanded, "init"]
        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe

        do {
            try process.run()
        } catch {
            return "Could not run hq init: \(error.localizedDescription)"
        }
        process.waitUntilExit()

        if process.terminationStatus == 0 {
            return nil
        }
        let errOutput = String(data: stderrPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let trimmed = errOutput.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? "hq init failed" : trimmed
    }

    /// Run `hq --dir <path> check` and return nil on success, error message on failure.
    nonisolated func validate(directory: String) -> String? {
        let expanded = Self.expandHome(directory)
        guard FileManager.default.isDirectory(atPath: expanded) else {
            return "Directory does not exist: \(expanded)"
        }
        let executableURL: URL
        do {
            executableURL = try Self.hqExecutableURL()
        } catch {
            return error.localizedDescription
        }

        let process = Process()
        process.executableURL = executableURL
        process.arguments = ["--dir", expanded, "check"]
        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe

        do {
            try process.run()
        } catch {
            return "Could not run hq check: \(error.localizedDescription)"
        }
        process.waitUntilExit()

        if process.terminationStatus == 0 {
            return nil
        }
        let errOutput = String(data: stderrPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let trimmed = errOutput.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? "hq check failed" : trimmed
    }

    nonisolated static func hqExecutableURL() throws -> URL {
        if let bundledURL = Bundle.main.resourceURL?.appendingPathComponent("hq"),
           FileManager.default.isExecutableFile(atPath: bundledURL.path)
        {
            return bundledURL
        }

        let fallback = expandHome("~/git/project-hq/target/release/hq")
        if FileManager.default.isExecutableFile(atPath: fallback) {
            return URL(fileURLWithPath: fallback)
        }

        throw AppError.missingExecutable
    }

    nonisolated private static func expandHome(_ path: String) -> String {
        if path == "~" {
            return NSHomeDirectory()
        }
        if path.hasPrefix("~/") {
            return NSHomeDirectory() + String(path.dropFirst())
        }
        return path
    }
}
