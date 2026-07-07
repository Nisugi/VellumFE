import SwiftUI

/// Boot state machine — the analog of `bootAndLoad` in MainActivity.kt:
/// install the password key, start the core (idempotent), health-poll the
/// local server, then hand the /play URL to the WebView.
@MainActor
final class BootModel: ObservableObject {
    enum Phase {
        case starting
        case ready(URL)
        case failed(String)
    }

    @Published var phase: Phase = .starting

    func boot() async {
        if case .ready = phase { return }
        phase = .starting

        let info = await Task.detached(priority: .userInitiated) { () -> CoreInfo in
            CryptoKeys.installPasswordKey()
            guard let dataDir = try? CoreBridge.dataDirectory() else {
                return CoreInfo(port: nil, token: nil, error: "Application Support directory unavailable")
            }
            return CoreBridge.startCore(dataDir: dataDir.path)
        }.value

        if let error = info.error {
            phase = .failed("Core failed to start:\n\(error)")
            return
        }
        guard let port = info.port, let token = info.token else {
            phase = .failed("Core returned an incomplete reply.")
            return
        }
        guard await waitForServer(port: port) else {
            phase = .failed("The embedded server did not come up on port \(port).")
            return
        }
        phase = .ready(URL(string: "http://127.0.0.1:\(port)/play#token=\(token)")!)
    }

    private func waitForServer(port: Int) async -> Bool {
        let health = URL(string: "http://127.0.0.1:\(port)/health")!
        var request = URLRequest(url: health)
        request.timeoutInterval = 0.5
        for _ in 0 ..< 40 { // ~10s
            if let (_, response) = try? await URLSession.shared.data(for: request),
               (response as? HTTPURLResponse)?.statusCode == 200 {
                return true
            }
            try? await Task.sleep(nanoseconds: 250_000_000)
        }
        return false
    }
}

struct ContentView: View {
    @StateObject private var model = BootModel()

    private static let background = Color(red: 0x11 / 255.0, green: 0x13 / 255.0, blue: 0x18 / 255.0)

    var body: some View {
        ZStack {
            Self.background.ignoresSafeArea()
            switch model.phase {
            case .starting:
                ProgressView()
                    .tint(Color(white: 0.6))
            case let .ready(url):
                WebViewContainer(url: url)
                    .ignoresSafeArea()
            case let .failed(message):
                // Same dark monospace error page the Android shell renders.
                VStack(alignment: .leading, spacing: 12) {
                    Text("VellumFE")
                        .font(.headline)
                        .foregroundColor(Color(red: 0.85, green: 0.33, blue: 0.31))
                    Text(message)
                        .font(.system(.callout, design: .monospaced))
                        .foregroundColor(Color(white: 0.84))
                        .textSelection(.enabled)
                }
                .padding(24)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
        }
        .preferredColorScheme(.dark)
        .task { await model.boot() }
    }
}
