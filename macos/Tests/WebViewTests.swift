import WebKit
import XCTest
@testable import HQDesktop

@MainActor
final class WebViewTests: XCTestCase {
    private final class RecordingWebView: WKWebView {
        var requests: [URLRequest] = []
        override var url: URL? { URL(string: "http://127.0.0.1:34567/") }
        override func load(_ request: URLRequest) -> WKNavigation? {
            requests.append(request)
            return nil
        }
    }

    func testSanitizedPageURLDoesNotCauseReloads() {
        let url = URL(string: "http://127.0.0.1:34567/?hq_token=first")!
        let coordinator = HQWebView.Coordinator(appURL: url)
        let webView = RecordingWebView()
        coordinator.loadIfNeeded(url, in: webView)
        coordinator.loadIfNeeded(url, in: webView)
        XCTAssertTrue(webView.requests.isEmpty)
    }

    func testWebContentRecoveryUsesLatestAuthenticatedURL() {
        let firstURL = URL(string: "http://127.0.0.1:34567/?hq_token=first")!
        let nextURL = URL(string: "http://127.0.0.1:45678/?hq_token=next")!
        let coordinator = HQWebView.Coordinator(appURL: firstURL)
        let webView = RecordingWebView()
        coordinator.webViewWebContentProcessDidTerminate(webView)
        coordinator.loadIfNeeded(nextURL, in: webView)
        coordinator.webViewWebContentProcessDidTerminate(webView)
        XCTAssertEqual(webView.requests.map(\.url), [firstURL, nextURL, nextURL])
    }
}
