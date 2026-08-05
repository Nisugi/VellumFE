import SwiftUI
import AVFoundation

/// The launch character picker: saved remote VellumFE servers (one per
/// character/port) plus "play on this phone". Add by scanning a `.webinfo`
/// QR in-app or entering host/port/token manually; delete per row; each row
/// shows a live/offline dot from a `/health` probe.
struct RemotePickerView: View {
    @ObservedObject var model: BootModel
    @State private var health: [String: Bool] = [:] // id → reachable
    @State private var showManual = false
    @State private var showScanner = false

    private static let background = Color(red: 0x11 / 255.0, green: 0x13 / 255.0, blue: 0x18 / 255.0)
    private static let accent = Color(red: 0xE5 / 255.0, green: 0xC0 / 255.0, blue: 0x7B / 255.0)

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                // Saved characters scroll in their own region so a long list
                // can never push the actions below off-screen.
                List {
                    Section {
                        if model.servers.isEmpty {
                            Text("No saved characters yet — scan a QR or add one manually.")
                                .foregroundColor(.secondary)
                                .font(.callout)
                        }
                        ForEach(model.servers) { server in
                            Button {
                                model.connectToSaved(server)
                            } label: {
                                row(server)
                            }
                        }
                        .onDelete { offsets in
                            for index in offsets {
                                model.deleteServer(id: model.servers[index].id)
                            }
                        }
                    }
                }

                // Always-visible actions: add entries, and the way back to the
                // login page (bottom corner — mirrors the Characters button's
                // spot on the login form).
                VStack(spacing: 10) {
                    Button {
                        showScanner = true
                    } label: {
                        Label("Scan QR to add", systemImage: "qrcode.viewfinder")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.bordered)
                    Button {
                        showManual = true
                    } label: {
                        Label("Add manually", systemImage: "keyboard")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.bordered)
                    HStack {
                        // Back to the login page ("play on this phone" is just
                        // going back now — the login form is the front door).
                        Button {
                            Task { await model.startLocal() }
                        } label: {
                            Image(systemName: "arrow.uturn.backward.circle.fill")
                                .font(.system(size: 30))
                        }
                        .accessibilityLabel("Back to login")
                        Spacer()
                    }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 10)
            }
            .navigationTitle("Characters")
            .toolbar { EditButton() }
        }
        .tint(Self.accent)
        .preferredColorScheme(.dark)
        .sheet(isPresented: $showManual) {
            ManualAddView { target in
                model.addServer(target)
                showManual = false
            }
        }
        .sheet(isPresented: $showScanner) {
            QRScannerView { scanned in
                showScanner = false
                if let target = Self.targetFromScan(scanned) {
                    model.addServer(target)
                }
            }
        }
        .task(id: model.servers.map(\.id)) {
            await probeAll()
        }
    }

    @ViewBuilder
    private func row(_ server: RemoteStore.Target) -> some View {
        HStack(spacing: 10) {
            Circle()
                .fill(statusColor(for: server))
                .frame(width: 9, height: 9)
            VStack(alignment: .leading, spacing: 2) {
                Text(server.name)
                    .foregroundColor(.primary)
                Text("\(server.host):\(server.port)")
                    .font(.system(.caption, design: .monospaced))
                    .foregroundColor(.secondary)
            }
            Spacer()
            Text(statusText(for: server))
                .font(.caption2)
                .foregroundColor(.secondary)
        }
    }

    private func statusColor(for server: RemoteStore.Target) -> Color {
        switch health[server.id] {
        case .some(true): return .green
        case .some(false): return Color(white: 0.4)
        case .none: return Color(white: 0.4)
        }
    }

    private func statusText(for server: RemoteStore.Target) -> String {
        switch health[server.id] {
        case .some(true): return "live"
        case .some(false): return "offline"
        case .none: return "…"
        }
    }

    /// Probe every saved server's /health endpoint concurrently.
    private func probeAll() async {
        await withTaskGroup(of: (String, Bool).self) { group in
            for server in model.servers {
                group.addTask {
                    (server.id, await Self.isReachable(server))
                }
            }
            for await (id, ok) in group {
                health[id] = ok
            }
        }
    }

    /// GET http://host:port/health with a short timeout. The endpoint is
    /// CORS-open and token-free (server.rs), so a 200 means the instance is up.
    private static func isReachable(_ server: RemoteStore.Target) async -> Bool {
        let host = server.host.contains(":") && !server.host.hasPrefix("[")
            ? "[\(server.host)]" : server.host
        guard let url = URL(string: "http://\(host):\(server.port)/health") else { return false }
        var request = URLRequest(url: url)
        request.timeoutInterval = 2.0
        guard let (_, response) = try? await URLSession.shared.data(for: request) else { return false }
        return (response as? HTTPURLResponse)?.statusCode == 200
    }

    /// Parse a scanned `vellum://remote?host&port&token&name` deep link into a
    /// saved-server entry. Returns nil for any other QR content.
    private static func targetFromScan(_ text: String) -> RemoteStore.Target? {
        guard let comps = URLComponents(string: text),
              comps.scheme == "vellum", comps.host == "remote",
              let items = comps.queryItems
        else { return nil }
        func value(_ name: String) -> String? {
            items.first { $0.name == name }?.value?.trimmingCharacters(in: .whitespaces)
        }
        guard let host = value("host"), !host.isEmpty,
              let portText = value("port"), let port = UInt16(portText), port > 0
        else { return nil }
        let name = value("name").flatMap { $0.isEmpty ? nil : $0 } ?? "\(host):\(port)"
        return RemoteStore.Target(name: name, host: host, port: Int(port), token: value("token") ?? "")
    }
}

/// Manual host/port/token entry for a saved server.
private struct ManualAddView: View {
    let onSave: (RemoteStore.Target) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var host = ""
    @State private var port = ""
    @State private var token = ""

    var body: some View {
        NavigationStack {
            Form {
                Section("Character") {
                    TextField("Label (required, e.g. Rysk)", text: $name)
                        .autocorrectionDisabled()
                }
                Section("Server") {
                    TextField("Host (e.g. 192.168.1.21)", text: $host)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                        .keyboardType(.URL)
                    TextField("Port (e.g. 8042)", text: $port)
                        .keyboardType(.numberPad)
                    TextField("Pairing token (optional)", text: $token)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                }
            }
            .navigationTitle("Add character")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        // Label is required: the picker lists entries by it.
                        guard let portNum = UInt16(port.trimmingCharacters(in: .whitespaces)),
                              portNum > 0,
                              !host.trimmingCharacters(in: .whitespaces).isEmpty,
                              !name.trimmingCharacters(in: .whitespaces).isEmpty
                        else { return }
                        let h = host.trimmingCharacters(in: .whitespaces)
                        onSave(RemoteStore.Target(
                            name: name.trimmingCharacters(in: .whitespaces),
                            host: h,
                            port: Int(portNum),
                            token: token.trimmingCharacters(in: .whitespaces)
                        ))
                    }
                    .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty
                        || host.trimmingCharacters(in: .whitespaces).isEmpty
                        || UInt16(port.trimmingCharacters(in: .whitespaces)) == nil)
            }
        }
        .preferredColorScheme(.dark)
    }
}

/// In-app camera QR scanner. Calls `onScan` with the decoded string once,
/// then the parent dismisses the sheet.
private struct QRScannerView: UIViewControllerRepresentable {
    let onScan: (String) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(onScan: onScan) }

    func makeUIViewController(context: Context) -> ScannerController {
        let controller = ScannerController()
        controller.onScan = { context.coordinator.handle($0) }
        return controller
    }

    func updateUIViewController(_ controller: ScannerController, context: Context) {}

    final class Coordinator {
        let onScan: (String) -> Void
        private var fired = false
        init(onScan: @escaping (String) -> Void) { self.onScan = onScan }
        /// Deliver the first successful scan only.
        func handle(_ text: String) {
            guard !fired else { return }
            fired = true
            onScan(text)
        }
    }
}

/// AVFoundation capture session that reports the first QR payload it reads.
final class ScannerController: UIViewController, AVCaptureMetadataOutputObjectsDelegate {
    var onScan: ((String) -> Void)?
    private let session = AVCaptureSession()
    private var preview: AVCaptureVideoPreviewLayer?

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black
        configure()
    }

    private func configure() {
        guard let device = AVCaptureDevice.default(for: .video),
              let input = try? AVCaptureDeviceInput(device: device),
              session.canAddInput(input)
        else { return }
        session.addInput(input)

        let output = AVCaptureMetadataOutput()
        guard session.canAddOutput(output) else { return }
        session.addOutput(output)
        output.setMetadataObjectsDelegate(self, queue: .main)
        output.metadataObjectTypes = [.qr]

        let layer = AVCaptureVideoPreviewLayer(session: session)
        layer.videoGravity = .resizeAspectFill
        layer.frame = view.layer.bounds
        view.layer.addSublayer(layer)
        preview = layer

        // Capturing on the main thread stalls the UI; start off it.
        DispatchQueue.global(qos: .userInitiated).async { [session] in
            session.startRunning()
        }
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        preview?.frame = view.layer.bounds
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        if session.isRunning {
            DispatchQueue.global(qos: .userInitiated).async { [session] in
                session.stopRunning()
            }
        }
    }

    func metadataOutput(
        _ output: AVCaptureMetadataOutput,
        didOutput objects: [AVMetadataObject],
        from connection: AVCaptureConnection
    ) {
        guard let obj = objects.first as? AVMetadataMachineReadableCodeObject,
              let text = obj.stringValue
        else { return }
        onScan?(text)
    }
}
