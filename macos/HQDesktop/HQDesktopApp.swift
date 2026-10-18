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
            if controller.needsSetup {
                WelcomeView(controller: controller)
            } else if controller.isReady, let appURL = controller.appURL {
                HQWebView(url: appURL)
            } else {
                VStack(spacing: 12) {
                    if !controller.canRetry {
                        ProgressView()
                            .controlSize(.large)
                    }
                    Text(controller.status)
                        .font(.headline)
                    Text(controller.detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                        .textSelection(.enabled)
                        .frame(maxWidth: 620)
                    if controller.canRetry {
                        Button("Retry") { controller.reload(directory: controller.hqDir) }
                    }
                }
                .padding(32)
            }
        }
    }
}

struct WelcomeView: View {
    @ObservedObject var controller: ServerController
    @State private var errorMessage: String?
    @State private var isWorking = false

    /// Where "Create My HQ Folder" puts the data. If HQ_DIR is set but missing,
    /// honor it instead of inventing a second location.
    private var createPath: String {
        controller.envOverrideActive ? controller.hqDir : "~/Documents/HQ"
    }

    var body: some View {
        VStack(spacing: 20) {
            VStack(spacing: 8) {
                Text("Welcome to HQ")
                    .font(.largeTitle.bold())
                Text("HQ is a project board that keeps everything as plain Markdown files in a folder you own — no accounts, no database.")
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 420)
            }

            VStack(spacing: 10) {
                Button {
                    createStarterFolder()
                } label: {
                    Text(isWorking ? "Setting up…" : "Create My HQ Folder")
                        .frame(minWidth: 220)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .disabled(isWorking)

                Text(createPath)
                    .font(.system(.caption, design: .monospaced))
                    .foregroundStyle(.secondary)

                Button("Choose an Existing Folder…") { chooseExistingFolder() }
                    .disabled(isWorking)
            }

            if let errorMessage {
                Text(errorMessage)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 420)
            }

            Text("Tip: because your HQ folder is plain Markdown, you can also open it as an Obsidian vault and do your longer writing there.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 420)
        }
        .padding(48)
    }

    private func createStarterFolder() {
        isWorking = true
        errorMessage = nil
        let path = createPath
        Task.detached { [controller] in
            // If the target is already a valid HQ directory (e.g. reinstall), adopt it as-is.
            let error = controller.validate(directory: path) == nil
                ? nil
                : controller.initialize(directory: path)
            await MainActor.run {
                isWorking = false
                if let error {
                    errorMessage = error
                } else {
                    controller.completeSetup(directory: path)
                }
            }
        }
    }

    private func chooseExistingFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = "Choose"

        guard panel.runModal() == .OK, let url = panel.url else { return }
        let path = url.path

        if let error = controller.validate(directory: path) {
            errorMessage = error
            return
        }

        errorMessage = nil
        controller.completeSetup(directory: path)
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
        context.coordinator.loadIfNeeded(url, in: webView)
    }

    final class Coordinator: NSObject, WKNavigationDelegate, WKUIDelegate {
        private var appURL: URL

        init(appURL: URL) {
            self.appURL = appURL
        }

        func loadIfNeeded(_ url: URL, in webView: WKWebView) {
            // The dashboard removes its token with history.replaceState.
            // Compare launch URLs, not the sanitized URL reported by WebKit.
            guard appURL != url else { return }
            appURL = url
            webView.load(URLRequest(url: url))
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
                if navigationAction.navigationType == .linkActivated && url.path != "/" {
                    decisionHandler(.cancel)
                    return
                }
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

        func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
            webView.load(URLRequest(url: appURL))
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
            return url.host == appURL.host && url.port == appURL.port
        }
    }
}

extension FileManager {
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
