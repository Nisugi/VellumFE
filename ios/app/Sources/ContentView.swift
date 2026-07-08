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

    private var port: Int?
    private var token: String?
    /// Fragment tail from a vellum:// deep link; rides the boot URL so the
    /// web client prefills the Lich login tab.
    private var lichFragment: String?

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
        self.port = port
        self.token = token
        phase = .ready(bootURL(port: port, token: token))
    }

    private func bootURL(port: Int, token: String) -> URL {
        var url = "http://127.0.0.1:\(port)/play#token=\(token)"
        if let lich = lichFragment {
            url += "&\(lich)"
        }
        return URL(string: url)!
    }

    /// vellum://lich?host=…&port=…[&name=…] → stash the target and, when
    /// the server is already up, republish the boot URL (the container
    /// reloads on change). Arriving mid-boot just rides along.
    func applyDeepLink(_ url: URL) {
        guard let fragment = Self.lichFragment(from: url) else { return }
        lichFragment = fragment
        if let port, let token {
            phase = .ready(bootURL(port: port, token: token))
        }
    }

    private static func lichFragment(from url: URL) -> String? {
        guard url.scheme == "vellum", url.host == "lich",
              let comps = URLComponents(url: url, resolvingAgainstBaseURL: false),
              let items = comps.queryItems
        else { return nil }
        func value(_ name: String) -> String? {
            items.first { $0.name == name }?.value?.trimmingCharacters(in: .whitespaces)
        }
        guard let host = value("host"), !host.isEmpty,
              let portText = value("port"), UInt16(portText).map({ $0 > 0 }) == true
        else { return nil }
        func encode(_ s: String) -> String {
            s.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? ""
        }
        var fragment = "lich=\(encode("\(host):\(portText)"))"
        if let name = value("name"), !name.isEmpty {
            fragment += "&name=\(encode(name))"
        }
        return fragment
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
        .onOpenURL { model.applyDeepLink($0) }
    }
}
