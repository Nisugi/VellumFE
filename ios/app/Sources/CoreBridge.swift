import Foundation

/// Reply envelope from `vellum_start_core` / `vellum_core_status`
/// (see ios/rust/include/vellum_core.h).
struct CoreInfo: Decodable {
    let port: Int?
    let token: String?
    let error: String?
}

/// Swift face of the Rust core's C ABI — the analog of Android's
/// `VellumCore.kt`. Every C string the core returns is copied into a Swift
/// `String` and freed immediately via `vellum_string_free`.
enum CoreBridge {
    /// Application Support/vellum — every config/profile/log path derives
    /// from this directory (it becomes VELLUM_FE_DIR inside the core).
    /// Included in device backups on purpose: config and profiles should
    /// survive a phone migration (sealed passwords won't decrypt on the new
    /// device — the Keychain key is device-bound — and fall back to re-entry).
    static func dataDirectory() throws -> URL {
        let base = try FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let dir = base.appendingPathComponent("vellum", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// Start the headless runtime (idempotent). Blocks briefly; call off
    /// the main thread.
    static func startCore(dataDir: String) -> CoreInfo {
        guard let raw = vellum_start_core(dataDir) else {
            return CoreInfo(port: nil, token: nil, error: "core returned no reply")
        }
        defer { vellum_string_free(raw) }
        return decode(String(cString: raw))
    }

    static func stopCore() {
        vellum_stop_core()
    }

    private static func decode(_ json: String) -> CoreInfo {
        do {
            return try JSONDecoder().decode(CoreInfo.self, from: Data(json.utf8))
        } catch {
            return CoreInfo(port: nil, token: nil, error: "unparseable core reply: \(json)")
        }
    }
}
