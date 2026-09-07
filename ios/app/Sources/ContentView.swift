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
    /// second CoreBridge.startCore or race its phase writes; it records the
    /// request in `requestedDestination` and shares the in-flight result.
    private var localBootInFlight = false

    /// The screen the user most recently asked for. Every navigation entry
    /// point (picker taps, deep links, shell URLs, local play) writes this
    /// BEFORE any async work, and boot completion consults it instead of a
    /// generation captured at boot start — so a local request made while an
    /// older, superseded boot is still in flight is not lost, and a boot
    /// finishing (or failing) after the user chose a remote server or the
    /// picker never steals the screen.
    private enum Destination: Equatable {
        case local
        case remote(RemoteStore.Target)
        case picker
    }
    private var requestedDestination: Destination = .local

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
        // Record the request FIRST, before deciding whether a new core start
        // is needed. `lichFragment` (if any) was stored by the caller before
        // this call, and `bootURL` reads it at render time, so the latest
        // deep-link prefill survives however the boot completes.
        requestedDestination = .local
        if let port, let token, coreStarted {
            // Core already up (returning from a remote view): just reload.
            allowedRemoteHost = nil
            phase = .ready(bootURL(port: port, token: token))
            return
        }
        if localBootInFlight {
            // A boot is already spinning up the core; don't start a second
            // one. Share its result: with the destination re-set to .local
            // above, its completion renders local play — even if a remote
            // navigation had superseded the boot in between. Show progress
            // meanwhile.
            phase = .starting
            return
        }
        localBootInFlight = true
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
        // the visible phase if local play is STILL the requested destination
        // when this boot finishes; a remote or picker choice made afterwards
        // stays authoritative, including over a boot failure.
        if let error = info.error {
            if requestedDestination == .local { phase = .failed("Core failed to start:\n\(error)") }
            return
        }
        guard let port = info.port, let token = info.token else {
            if requestedDestination == .local { phase = .failed("Core returned an incomplete reply.") }
            return
        }
        guard await waitForServer(port: port) else {
            if requestedDestination == .local {
                phase = .failed("The embedded server did not come up on port \(port).")
            }
            return
        }
        self.port = port
        self.token = token
        coreStarted = true
        guard requestedDestination == .local else {
            // A remote / picker navigation superseded this boot; leave the
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
        requestedDestination = .picker // supersede any in-flight local boot
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
        if let chars = Self.charsFragment() {
            url += "&\(chars)"
        }
        if let lich = lichFragment {
            url += "&\(lich)"
        }
        return URL(string: url)!
    }

    /// The saved characters as `chars=` + `charids=` fragment params for
    /// the web client's switch-character wheel: display `name@host:port`
    /// entries plus each target's stable store ID (same order). A wheel
    /// pick round-trips through vellum://remote/connect?id=… and is
    /// resolved BY ID — names are display only, so duplicate labels at
    /// different servers stay distinct. Pairing tokens stay in the
    /// Keychain; this shell connects with its own stored token. Nil when
    /// nothing is saved. (Older web clients ignore the unknown charids=
    /// param and keep their name-based behavior.)
    private static func charsFragment() -> String? {
        let targets = RemoteStore.list()
        if targets.isEmpty { return nil }
        let entries = targets.map { target in
            "\(encode(target.name))@\(encode(target.host)):\(target.port)"
        }
        let ids = targets.map { encode($0.id) }
        return "chars=" + entries.joined(separator: ",")
            + "&charids=" + ids.joined(separator: ",")
    }

    /// Local play requested from the page (vellum://local). Routes through
    /// `startLocal()` so every local-navigation entry point — picker action,
    /// deep link, shell URL — shares one state transition: the request is
    /// recorded even while a boot is in flight, and the running core (or the
    /// shared in-flight boot) is reused rather than restarted.
    private func showLocal() {
        Task { await startLocal() }
    }

    /// Point the WebView at a desktop VellumFE's dashboard. The embedded
    /// core keeps running but sits idle — there is no in-app game socket
    /// in this mode; the web client's own reconnect handles resume.
    private func showRemote(_ target: RemoteStore.Target) {
        // A remote request supersedes an in-flight local boot and stays
        // authoritative until another navigation request replaces it.
        requestedDestination = .remote(target)
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
        // The saved target's stable store ID rides the fragment so a pairing
        // token entered on the remote dashboard (a tokenless manual entry)
        // can round-trip back through vellum://remote/token and update THIS
        // exact entry — by ID, never by name/host guessing.
        url += "&sid=\(Self.encode(target.id))"
        if let chars = Self.charsFragment() {
            url += "&\(chars)"
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
            case "/connect":
                // Switch-character wheel pick: connect to a saved server by
                // its stable store ID (the token comes from the Keychain
                // entry, never the page). An explicit ID never falls back
                // to a name match; legacy name-only requests resolve only
                // when exactly one entry matches. Anything unknown,
                // deleted, or ambiguous lands on the picker — never on a
                // different entry.
                if let target = Self.resolveConnect(
                    RemoteStore.list(),
                    id: Self.queryValue(url, "id"),
                    name: Self.queryValue(url, "name")
                ) {
                    showRemote(target)
                } else {
                    showPicker()
                }
            case "/token":
                // A pairing token the remote dashboard accepted for a saved
                // server that lacked one: persist it on exactly the entry
                // named by its stable store ID. No navigation — the page
                // already retried with the accepted token and is showing
                // the session list.
                if let id = Self.queryValue(url, "id"), !id.isEmpty,
                   let token = Self.queryValue(url, "token"), !token.isEmpty {
                    RemoteStore.updateToken(id: id, token: token)
                    servers = RemoteStore.list()
                }
            default:
                break
            }
        default:
            break
        }
    }

    /// Resolve a vellum://remote/connect pick (proposal item 7). A
    /// non-blank `id` matches only by ID — never falling back to a name —
    /// so a stale selection can't be redirected to a different server
    /// after a delete. A legacy name-only request resolves only when
    /// exactly ONE saved entry carries that name. Nil = show the picker.
    /// Mirrors the Android shell's `CharacterWheel.resolve`.
    static func resolveConnect(
        _ targets: [RemoteStore.Target],
        id: String?,
        name: String?
    ) -> RemoteStore.Target? {
        if let id, !id.isEmpty {
            return targets.first { $0.id == id }
        }
        if let name, !name.isEmpty {
            let matches = targets.filter { $0.name == name }
            return matches.count == 1 ? matches[0] : nil
        }
        return nil
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
