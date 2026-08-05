import SwiftUI

/// Boot state machine — the analog of `bootAndLoad` in MainActivity.kt:
/// install the password key, start the core (idempotent), health-poll the
/// local server, then hand the /play URL to the WebView.
@MainActor
final class BootModel: ObservableObject {
    enum Phase {
        case starting
        /// The native character picker: saved remote servers + "play on this
        /// phone". Shown at launch when at least one server is saved.
        case picker
        case ready(URL)
        case failed(String)
    }

    @Published var phase: Phase = .starting

    /// Saved remote servers for the picker. Published so the picker view
    /// re-renders after an add/delete.
    @Published private(set) var servers: [RemoteStore.Target] = []

    /// True once the embedded core is up (local play started). Lets the
    /// picker offer "play on this phone" without restarting the core.
    private var coreStarted = false

    /// True while a `startLocal()` core-boot is in flight. A second call
    /// (e.g. a vellum://lich deep link landing mid-launch) must not spawn a
    /// second CoreBridge.startCore or race its phase writes.
    private var localBootInFlight = false

    /// Bumped whenever the user navigates away from an in-flight local boot
    /// (a remote deep link, or returning to the picker). A boot that started
    /// before the bump must not overwrite the newer phase when it finishes.
    private var navGeneration = 0

    /// Remote server the WebView is allowed to browse in-app (Remote mode);
    /// nil while on the embedded core. Read by the container's nav policy.
    @Published private(set) var allowedRemoteHost: String?

    private var port: Int?
    private var token: String?
    /// Fragment tail from a vellum://lich deep link; rides the boot URL so
    /// the web client prefills the Lich login tab. (Remote deep links no
    /// longer prefill the page — the native picker owns remote servers.)
    private var lichFragment: String?

    func boot() async {
        if case .ready = phase { return }
        if case .picker = phase { return }
        servers = RemoteStore.list()

        // Land on the login page (local play) at launch — the New-login form
        // is the app's front door. The native character picker (saved remote
        // servers, Scan QR, Add manually) stays reachable from the in-page
        // Characters button (vellum://remote/picker), so nothing is lost;
        // it's just no longer the landing screen. A lich deep link arriving
        // at launch rides along: startLocal() picks up lichFragment and the
        // web client prefills the Lich tab.
        await startLocal()
    }

    /// Start the embedded core (idempotent) and load local play. Used at
    /// launch when no servers are saved, and from the picker's "play on this
    /// phone" entry.
    func startLocal() async {
        if let port, let token, coreStarted {
            // Core already up (returning from a remote view): just reload.
            allowedRemoteHost = nil
            phase = .ready(bootURL(port: port, token: token))
            return
        }
        // A boot is already spinning up the core; don't start a second one.
        // The in-flight boot will land on local play; a deep link that wants
        // something else will have bumped navGeneration and win.
        if localBootInFlight { return }
        localBootInFlight = true
        let generation = navGeneration
        defer { localBootInFlight = false }

        phase = .starting
        let info = await Task.detached(priority: .userInitiated) { () -> CoreInfo in
            CryptoKeys.installPasswordKey()
            guard let dataDir = try? CoreBridge.dataDirectory() else {
                return CoreInfo(port: nil, token: nil, error: "Application Support directory unavailable")
            }
            return CoreBridge.startCore(dataDir: dataDir.path)
        }.value

        // The core is a process-wide singleton, so record its port/token even
        // if the user navigated away mid-boot — a later "play on this phone"
        // must reuse it rather than start a second core. But only take over
        // the visible phase if nothing newer (a remote deep link, the picker)
        // superseded this boot while it was running.
        if let error = info.error {
            if generation == navGeneration { phase = .failed("Core failed to start:\n\(error)") }
            return
        }
        guard let port = info.port, let token = info.token else {
            if generation == navGeneration { phase = .failed("Core returned an incomplete reply.") }
            return
        }
        guard await waitForServer(port: port) else {
            if generation == navGeneration {
                phase = .failed("The embedded server did not come up on port \(port).")
            }
            return
        }
        self.port = port
        self.token = token
        coreStarted = true
        guard generation == navGeneration else {
            // A deep link / picker navigation superseded this boot; leave the
            // newer phase in place. The core is ready for later reuse.
            return
        }
        allowedRemoteHost = nil
        phase = .ready(bootURL(port: port, token: token))
    }

    // MARK: - Picker actions

    /// Connect to a saved server (picker tap).
    func connectToSaved(_ target: RemoteStore.Target) {
        showRemote(target)
    }

    /// Add a server from the manual form or a scanned QR, then refresh the
    /// picker list (staying on the picker so more can be added).
    func addServer(_ target: RemoteStore.Target) {
        RemoteStore.add(target)
        servers = RemoteStore.list()
    }

    /// Delete a saved server by id and refresh the list.
    func deleteServer(id: String) {
        servers = RemoteStore.remove(id: id)
    }

    /// Return to the picker from a remote/local view.
    func showPicker() {
        navGeneration += 1 // supersede any in-flight local boot
        allowedRemoteHost = nil
        servers = RemoteStore.list()
        phase = .picker
    }

    private func bootURL(port: Int, token: String) -> URL {
        // app=1 marks the shell for the web client. nativepicker=1 tells it a
        // native character picker owns remote-server management, so the
        // in-page Remote login tab is hidden (a plain browser, lacking both,
        // keeps its Remote tab).
        var url = "http://127.0.0.1:\(port)/play#token=\(token)&app=1&nativepicker=1"
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
        navGeneration += 1 // a remote deep link supersedes an in-flight local boot
        allowedRemoteHost = target.host.lowercased()
        // Bracket bare IPv6 literals so the URL parses.
        let host = target.host.contains(":") && !target.host.hasPrefix("[")
            ? "[\(target.host)]" : target.host
        // nativepicker=1: hide the web client's in-page Remote tab; the
        // native picker (with a Back affordance) owns switching servers.
        var url = "http://\(host):\(target.port)/#app=1&nativepicker=1"
        if !target.token.isEmpty {
            url += "&token=\(target.token)"
        }
        guard let parsed = URL(string: url) else { return }
        phase = .ready(parsed)
    }

    /// An OS-delivered deep link (a .webinfo QR scanned with the system
    /// camera, which launches the app):
    ///  - vellum://lich?…  → prefill the web client's Lich tab, then local play.
    ///  - vellum://remote?… → add the character to the saved list and connect
    ///    (the native picker owns remote servers now).
    func applyDeepLink(_ url: URL) {
        if url.scheme == "vellum", url.host == "remote" {
            guard let target = Self.remoteTarget(from: url) else { return }
            if Self.queryValue(url, "save") != "0" {
                addServer(target)
            }
            showRemote(target)
            return
        }
        if let fragment = Self.lichFragment(from: url) {
            lichFragment = fragment
            Task { await startLocal() }
        }
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
                // Pair: vellum://remote?host&port[&token][&name][&save=0].
                // Reached when the OS camera scans a .webinfo QR and hands the
                // deep link to the app. Add to the saved list (unless save=0),
                // then connect.
                guard let target = Self.remoteTarget(from: url) else { return }
                if Self.queryValue(url, "save") != "0" {
                    addServer(target)
                }
                showRemote(target)
            case "/picker":
                // Return to the native character picker.
                showPicker()
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
        // The character name rides the extended .webinfo deep link so a
        // scanned entry auto-names itself; fall back to host:port.
        let name = queryValue(url, "name").flatMap { $0.isEmpty ? nil : $0 }
            ?? "\(host):\(port)"
        return RemoteStore.Target(
            name: name,
            host: host,
            port: Int(port),
            token: queryValue(url, "token") ?? ""
        )
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
            case .picker:
                RemotePickerView(model: model)
            case let .ready(url):
                WebViewContainer(
                    url: url,
                    allowedHost: model.allowedRemoteHost,
                    onShellURL: { model.handleShellURL($0) }
                )
                .ignoresSafeArea()
                // The route back to the native character picker is now the
                // web login form's own Characters button (bottom action row,
                // vellum://remote/picker) — no floating native overlay. The
                // shell intercepts vellum:// on loopback and remote pages
                // alike, so both modes have a way home.
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
