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

    /// Remote server the WebView is allowed to browse in-app (Remote mode);
    /// nil while on the embedded core. Read by the container's nav policy.
    @Published private(set) var allowedRemoteHost: String?

    private var port: Int?
    private var token: String?
    /// Fragment tail from a vellum:// deep link; rides the boot URL so the
    /// web client prefills the Lich login tab.
    private var lichFragment: String?
    /// Prefill tail from a vellum://remote deep link (rhost/rport/rkey);
    /// rides the boot URL so the web client opens the Remote tab. Never
    /// auto-connects — the user presses Connect on the login page.
    private var remoteFragment: String?
    /// The remembered remote server (Keychain). Its address (never the
    /// token) rides the boot URL so the login page can offer one-tap
    /// connect; Connect/Forget come back as vellum://remote/… actions.
    private var savedRemote: RemoteStore.Target?

    func boot() async {
        if case .ready = phase { return }
        phase = .starting
        savedRemote = RemoteStore.load()

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
        // app=1 marks the shell for the web client: it reveals the Remote
        // login tab (whose actions only a shell can catch).
        var url = "http://127.0.0.1:\(port)/play#token=\(token)&app=1"
        if let saved = savedRemote {
            url += "&remote=\(Self.encode("\(saved.host):\(saved.port)"))"
        }
        if let remote = remoteFragment {
            url += "&\(remote)"
        }
        if let lich = lichFragment {
            url += "&\(lich)"
        }
        return URL(string: url)!
    }

    /// Republish the local boot URL (the container reloads on change).
    /// No-op while boot is still in flight — it picks the fragments up.
    private func showLocal() {
        allowedRemoteHost = nil
        if let port, let token {
            phase = .ready(bootURL(port: port, token: token))
        }
    }

    /// Point the WebView at a desktop VellumFE's dashboard. The embedded
    /// core keeps running but sits idle — there is no in-app game socket
    /// in this mode; the web client's own reconnect handles resume.
    private func showRemote(_ target: RemoteStore.Target) {
        allowedRemoteHost = target.host.lowercased()
        // Bracket bare IPv6 literals so the URL parses.
        let host = target.host.contains(":") && !target.host.hasPrefix("[")
            ? "[\(target.host)]" : target.host
        var url = "http://\(host):\(target.port)/#"
        url += target.token.isEmpty ? "app=1" : "token=\(target.token)&app=1"
        guard let parsed = URL(string: url) else { return }
        phase = .ready(parsed)
    }

    /// vellum://lich?host=…&port=…[&name=…] and
    /// vellum://remote?host=…&port=…[&token=…] → stash a prefill fragment
    /// and republish the boot URL. Both are prefill-only: a malicious QR
    /// can't point the app at an attacker's socket unseen.
    func applyDeepLink(_ url: URL) {
        if let fragment = Self.lichFragment(from: url) {
            lichFragment = fragment
        } else if let fragment = Self.remoteFragment(from: url) {
            remoteFragment = fragment
        } else {
            return
        }
        showLocal()
    }

    /// vellum:// navigations from the page itself (Remote tab actions).
    func handleShellURL(_ url: URL) {
        guard url.scheme == "vellum" else { return }
        switch url.host {
        case "local":
            showLocal()
        case "remote":
            switch url.path {
            case "", "/":
                // Pair: vellum://remote?host&port[&token][&save=0]
                guard let target = Self.remoteTarget(from: url) else { return }
                if Self.queryValue(url, "save") != "0" {
                    RemoteStore.save(target)
                    savedRemote = target
                }
                showRemote(target)
            case "/connect":
                if let target = savedRemote {
                    showRemote(target)
                }
            case "/forget":
                RemoteStore.forget()
                savedRemote = nil
                showLocal()
            default:
                break
            }
        default:
            break
        }
    }

    private static func queryValue(_ url: URL, _ name: String) -> String? {
        URLComponents(url: url, resolvingAgainstBaseURL: false)?
            .queryItems?
            .first { $0.name == name }?
            .value?
            .trimmingCharacters(in: .whitespaces)
    }

    private static func remoteTarget(from url: URL) -> RemoteStore.Target? {
        guard let host = queryValue(url, "host"), !host.isEmpty,
              let portText = queryValue(url, "port"),
              let port = UInt16(portText), port > 0
        else { return nil }
        return RemoteStore.Target(
            host: host,
            port: Int(port),
            token: queryValue(url, "token") ?? ""
        )
    }

    /// vellum://remote?… → the #rhost=…&rport=…[&rkey=…] prefill tail.
    /// ("rkey": the web client's token regex is unanchored, so any
    /// *token= param in the local fragment would be eaten by it.)
    private static func remoteFragment(from url: URL) -> String? {
        guard url.host == "remote", url.path.isEmpty || url.path == "/",
              let target = remoteTarget(from: url)
        else { return nil }
        var fragment = "rhost=\(encode(target.host))&rport=\(target.port)"
        if !target.token.isEmpty {
            fragment += "&rkey=\(encode(target.token))"
        }
        return fragment
    }

    private static func encode(_ s: String) -> String {
        s.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? ""
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
                WebViewContainer(
                    url: url,
                    allowedHost: model.allowedRemoteHost,
                    onShellURL: { model.handleShellURL($0) }
                )
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
