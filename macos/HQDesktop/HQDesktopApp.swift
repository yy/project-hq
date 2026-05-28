import Combine
import Foundation
import SwiftUI
import WebKit

@main
struct HQDesktopApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    var body: some Scene {
        WindowGroup {
            ContentView(controller: appDelegate.controller)
                .frame(minWidth: 760, minHeight: 560)
        }
        .commands {
            CommandGroup(replacing: .newItem) {}
        }

        Settings {
            SettingsView(controller: appDelegate.controller)
        }
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let controller = ServerController()

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
        controller.start()
    }

    func applicationWillTerminate(_ notification: Notification) {
        controller.stop()
    }
}

@MainActor
final class ServerController: ObservableObject {
    static let dataDirDefaultsKey = "HQDataDir"

    @Published private(set) var isReady = false
    @Published private(set) var status = "Starting HQ..."
    @Published private(set) var detail = ""
    @Published private(set) var hqDir: String

    let port: Int
    let appURL: URL
    let envOverrideActive: Bool

    private var process: Process?
    private var stdoutPipe: Pipe?
    private var stderrPipe: Pipe?
    private var didStart = false
    private var ownsServer = false

    init() {
        let environment = ProcessInfo.processInfo.environment
        let bundle = Bundle.main
        let bundledPort = bundle.object(forInfoDictionaryKey: "HQPort") as? String
        let bundledHQDir = bundle.object(forInfoDictionaryKey: "HQDataDir") as? String
        self.port = Int(environment["HQ_DESKTOP_PORT"] ?? bundledPort ?? "") ?? 3001
        self.appURL = URL(string: "http://127.0.0.1:\(port)/")!

        let envDir = environment["HQ_DIR"].flatMap { $0.isEmpty ? nil : $0 }
        self.envOverrideActive = envDir != nil
        let savedDir = UserDefaults.standard.string(forKey: Self.dataDirDefaultsKey)
        let resolved = envDir
            ?? savedDir
            ?? bundledHQDir
            ?? "~/git/hq"
        self.hqDir = Self.expandHome(resolved)
    }

    func start() {
        guard !didStart else {
            return
        }
        didStart = true

        Task {
            if await waitForServer(timeout: 0.6) {
                status = "Connected to existing HQ server"
                detail = appURL.absoluteString
                isReady = true
                return
            }

            do {
                try launchServer()
                status = "Starting local HQ server..."

                if await waitForServer(timeout: 8.0) {
                    status = "HQ is ready"
                    detail = appURL.absoluteString
                    isReady = true
                } else {
                    status = "HQ server did not become ready"
                    detail = "Expected \(appURL.absoluteString)"
                }
            } catch {
                status = "Could not start HQ"
                detail = error.localizedDescription
            }
        }
    }

    func stop() {
        guard ownsServer, let process else {
            return
        }
        if process.isRunning {
            process.terminate()
        }
    }

    /// Switch the server to a new data directory. Always owns the server after this call.
    /// Caller should validate the directory first (see `validate(directory:)`).
    func reload(directory: String) {
        let expanded = Self.expandHome(directory)

        if let process, process.isRunning {
            process.terminate()
            process.waitUntilExit()
        }
        self.process = nil
        self.stdoutPipe = nil
        self.stderrPipe = nil
        self.ownsServer = false

        self.hqDir = expanded
        self.isReady = false
        self.status = "Reloading HQ in \(expanded)..."
        self.detail = ""

        Task {
            do {
                try launchServer()
                if await waitForServer(timeout: 8.0) {
                    status = "HQ is ready"
                    detail = appURL.absoluteString
                    isReady = true
                } else {
                    status = "HQ server did not become ready"
                    detail = "Expected \(appURL.absoluteString)"
                }
            } catch {
                status = "Could not start HQ"
                detail = error.localizedDescription
            }
        }
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

    private func launchServer() throws {
        let executableURL = try hqExecutableURL()
        guard FileManager.default.isDirectory(atPath: hqDir) else {
            throw AppError.missingHQDirectory(hqDir)
        }

        let process = Process()
        process.executableURL = executableURL
        process.currentDirectoryURL = URL(fileURLWithPath: hqDir)
        process.arguments = ["--dir", hqDir, "serve", "--port", "\(port)"]

        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe
        self.stdoutPipe = stdoutPipe
        self.stderrPipe = stderrPipe

        stdoutPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            Task { @MainActor in
                self?.captureOutput(data)
            }
        }
        stderrPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            Task { @MainActor in
                self?.captureOutput(data)
            }
        }

        process.terminationHandler = { [weak self] process in
            Task { @MainActor in
                guard let self, self.ownsServer, !self.isReady else {
                    return
                }
                self.status = "HQ server exited"
                self.detail = "Exit status \(process.terminationStatus)"
            }
        }

        try process.run()
        self.process = process
        ownsServer = true
    }

    private func captureOutput(_ data: Data) {
        guard !data.isEmpty, let message = String(data: data, encoding: .utf8) else {
            return
        }
        Task { @MainActor in
            self.detail = message.trimmingCharacters(in: .whitespacesAndNewlines)
        }
    }

    private func waitForServer(timeout: TimeInterval) async -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if await canReachServer() {
                return true
            }
            try? await Task.sleep(nanoseconds: 200_000_000)
        }
        return false
    }

    private func canReachServer() async -> Bool {
        do {
            var request = URLRequest(url: appURL)
            request.timeoutInterval = 0.5
            let (_, response) = try await URLSession.shared.data(for: request)
            if let httpResponse = response as? HTTPURLResponse {
                return (200..<500).contains(httpResponse.statusCode)
            }
            return false
        } catch {
            return false
        }
    }

    private func hqExecutableURL() throws -> URL {
        try Self.hqExecutableURL()
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

enum AppError: LocalizedError {
    case missingExecutable
    case missingHQDirectory(String)

    var errorDescription: String? {
        switch self {
        case .missingExecutable:
            return "Could not find bundled hq executable. Rebuild the app with script/build_and_run.sh."
        case .missingHQDirectory(let path):
            return "HQ data directory does not exist: \(path)"
        }
    }
}

struct ContentView: View {
    @ObservedObject var controller: ServerController

    var body: some View {
        Group {
            if controller.isReady {
                HQWebView(url: controller.appURL)
            } else {
                VStack(spacing: 12) {
                    ProgressView()
                        .controlSize(.large)
                    Text(controller.status)
                        .font(.headline)
                    Text(controller.detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                        .textSelection(.enabled)
                        .frame(maxWidth: 620)
                }
                .padding(32)
            }
        }
    }
}

struct HQWebView: NSViewRepresentable {
    let url: URL

    func makeCoordinator() -> Coordinator {
        Coordinator(appURL: url)
    }

    func makeNSView(context: Context) -> WKWebView {
        let webView = WKWebView(frame: .zero, configuration: WKWebViewConfiguration())
        webView.navigationDelegate = context.coordinator
        webView.uiDelegate = context.coordinator
        webView.load(URLRequest(url: url))
        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        if webView.url != url {
            webView.load(URLRequest(url: url))
        }
    }

    final class Coordinator: NSObject, WKNavigationDelegate, WKUIDelegate {
        let appHost: String?
        let appPort: Int?

        init(appURL: URL) {
            self.appHost = appURL.host
            self.appPort = appURL.port
        }

        func webView(
            _ webView: WKWebView,
            decidePolicyFor navigationAction: WKNavigationAction,
            decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
        ) {
            guard let url = navigationAction.request.url else {
                decisionHandler(.allow)
                return
            }

            if isInternal(url) {
                decisionHandler(.allow)
                return
            }

            if navigationAction.navigationType == .linkActivated || navigationAction.targetFrame == nil {
                NSWorkspace.shared.open(url)
                decisionHandler(.cancel)
                return
            }

            decisionHandler(.allow)
        }

        func webView(
            _ webView: WKWebView,
            createWebViewWith configuration: WKWebViewConfiguration,
            for navigationAction: WKNavigationAction,
            windowFeatures: WKWindowFeatures
        ) -> WKWebView? {
            if let url = navigationAction.request.url {
                if isInternal(url) {
                    webView.load(navigationAction.request)
                } else {
                    NSWorkspace.shared.open(url)
                }
            }
            return nil
        }

        private func isInternal(_ url: URL) -> Bool {
            guard let scheme = url.scheme?.lowercased() else {
                return true
            }
            if scheme == "about" || scheme == "data" || scheme == "blob" {
                return true
            }
            guard scheme == "http" || scheme == "https" else {
                return false
            }
            return url.host == appHost && url.port == appPort
        }
    }
}

private extension FileManager {
    func isDirectory(atPath path: String) -> Bool {
        var isDirectory: ObjCBool = false
        return fileExists(atPath: path, isDirectory: &isDirectory) && isDirectory.boolValue
    }
}

struct SettingsView: View {
    @ObservedObject var controller: ServerController
    @AppStorage(ServerController.dataDirDefaultsKey) private var savedDir: String = ""
    @State private var errorMessage: String?

    var body: some View {
        Form {
            Section("HQ Directory") {
                if controller.envOverrideActive {
                    Text("HQ_DIR environment variable is set; it takes precedence over this setting until the app is relaunched without it.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                HStack {
                    Text(controller.hqDir)
                        .font(.system(.body, design: .monospaced))
                        .textSelection(.enabled)
                        .lineLimit(1)
                        .truncationMode(.head)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Button("Choose…") { chooseDirectory() }
                }

                if let errorMessage {
                    Text(errorMessage)
                        .font(.caption)
                        .foregroundStyle(.red)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .padding(20)
        .frame(width: 520)
    }

    private func chooseDirectory() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = "Choose"
        if FileManager.default.isDirectory(atPath: controller.hqDir) {
            panel.directoryURL = URL(fileURLWithPath: controller.hqDir)
        }

        guard panel.runModal() == .OK, let url = panel.url else { return }
        let path = url.path

        if let error = controller.validate(directory: path) {
            errorMessage = error
            return
        }

        errorMessage = nil
        if !controller.envOverrideActive {
            savedDir = path
        }
        controller.reload(directory: path)
    }
}
