import SwiftUI
import UIKit
import WebKit

/// Fullscreen WKWebView over the embedded web frontend — the analog of the
/// Android shell's WebView block in `MainActivity.kt`. The web UI owns safe
/// areas (env(safe-area-inset-*)) and the soft keyboard (--vvh), so the
/// view extends edge to edge and never applies its own insets.
struct WebViewContainer: UIViewRepresentable {
    let url: URL

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeUIView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        config.allowsInlineMediaPlayback = true
        // Sound alerts fire from JS without a user gesture.
        config.mediaTypesRequiringUserActionForPlayback = []
        // Persistent localStorage; the pairing token also always re-arrives
        // via the #token= URL fragment, so eviction self-heals.
        config.websiteDataStore = .default()

        let webView = WKWebView(frame: .zero, configuration: config)
        webView.navigationDelegate = context.coordinator
        webView.uiDelegate = context.coordinator
        webView.isOpaque = false
        webView.backgroundColor = UIColor(red: 0x11 / 255.0, green: 0x13 / 255.0, blue: 0x18 / 255.0, alpha: 1)
        webView.scrollView.backgroundColor = webView.backgroundColor
        webView.scrollView.contentInsetAdjustmentBehavior = .never
        webView.scrollView.bounces = false
        webView.allowsBackForwardNavigationGestures = false
        #if DEBUG
            // Safari Web Inspector from a paired Mac (Develop menu).
            if #available(iOS 16.4, *) {
                webView.isInspectable = true
            }
        #endif
        webView.load(URLRequest(url: url))
        return webView
    }

    func updateUIView(_ webView: WKWebView, context: Context) {
        // The boot URL is fixed for the app's lifetime; nothing to update.
    }

    final class Coordinator: NSObject, WKNavigationDelegate, WKUIDelegate {
        /// Everything except the local server goes to Safari (game
        /// LaunchURL links, play.net pages) — mirrors
        /// `shouldOverrideUrlLoading` in MainActivity.kt.
        func webView(
            _ webView: WKWebView,
            decidePolicyFor navigationAction: WKNavigationAction,
            decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
        ) {
            guard let url = navigationAction.request.url else {
                decisionHandler(.allow)
                return
            }
            if url.host == "127.0.0.1" || !["http", "https"].contains(url.scheme ?? "") {
                decisionHandler(.allow)
            } else {
                UIApplication.shared.open(url)
                decisionHandler(.cancel)
            }
        }

        /// window.open / target=_blank has no target frame here; treat it
        /// as an external link.
        func webView(
            _ webView: WKWebView,
            createWebViewWith configuration: WKWebViewConfiguration,
            for navigationAction: WKNavigationAction,
            windowFeatures: WKWindowFeatures
        ) -> WKWebView? {
            if let url = navigationAction.request.url {
                UIApplication.shared.open(url)
            }
            return nil
        }
    }
}
