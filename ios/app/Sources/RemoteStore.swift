import Foundation
import Security

/// The remembered remote VellumFE servers (one per character/port) for the
/// Remote character picker, stored as a single Keychain item holding a JSON
/// array — same trust posture as the master password key in `CryptoKeys.swift`:
/// device-bound, never in backups. Mirrors the Android shell's `RemoteStore.kt`.
///
/// History: this used to hold exactly one `Target`. `load()` migrates that
/// legacy single-item blob into a one-element list on first read, so upgrades
/// keep their saved server.
enum RemoteStore {
    /// One saved remote server. `id` is a stable local handle for delete;
    /// `name` is the display label (the character name from a scanned QR, or
    /// host:port for a manual entry).
    struct Target: Codable, Equatable, Identifiable {
        var id: String
        var name: String
        var host: String
        var port: Int
        /// Pairing token for that PC's web server; empty when the user
        /// paired without one (the remote page prompts instead).
        var token: String

        init(id: String = UUID().uuidString, name: String, host: String, port: Int, token: String) {
            self.id = id
            self.name = name
            self.host = host
            self.port = port
            self.token = token
        }
    }

    /// Legacy single-item shape (no id/name) for one-time migration.
    private struct LegacyTarget: Codable {
        var host: String
        var port: Int
        var token: String
    }

    private static let base: [String: Any] = [
        kSecClass as String: kSecClassGenericPassword,
        kSecAttrService as String: "dev.vellumfe.remote-server",
        kSecAttrAccount as String: "vellum-remote",
    ]

    // MARK: - List API

    /// All saved servers, newest last. Migrates a legacy single-item blob.
    static func list() -> [Target] {
        guard let data = readBlob() else { return [] }
        if let targets = try? JSONDecoder().decode([Target].self, from: data) {
            return targets
        }
        // Legacy single Target (pre-picker): wrap it into a one-element list
        // and persist the migrated form so later reads take the fast path.
        if let legacy = try? JSONDecoder().decode(LegacyTarget.self, from: data) {
            let migrated = Target(
                name: "\(legacy.host):\(legacy.port)",
                host: legacy.host,
                port: legacy.port,
                token: legacy.token
            )
            writeBlob([migrated])
            return [migrated]
        }
        return []
    }

    /// Add a server, or replace one already saved for the same host:port
    /// (re-scanning a character updates its token/name rather than duplicating).
    static func add(_ target: Target) {
        var targets = list()
        if let idx = targets.firstIndex(where: {
            $0.host.caseInsensitiveCompare(target.host) == .orderedSame && $0.port == target.port
        }) {
            // Keep the existing id (stable handle), take the new name/token.
            var updated = target
            updated.id = targets[idx].id
            targets[idx] = updated
        } else {
            targets.append(target)
        }
        writeBlob(targets)
    }

    /// Remove one saved server by id. Returns the remaining list.
    @discardableResult
    static func remove(id: String) -> [Target] {
        var targets = list()
        targets.removeAll { $0.id == id }
        writeBlob(targets)
        return targets
    }

    // MARK: - Keychain blob

    private static func readBlob() -> Data? {
        var query = base
        query[kSecReturnData as String] = true
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
              let data = item as? Data
        else { return nil }
        return data
    }

    private static func writeBlob(_ targets: [Target]) {
        // An empty list clears the item entirely (no stray blob left behind).
        guard !targets.isEmpty else {
            SecItemDelete(base as CFDictionary)
            return
        }
        guard let data = try? JSONEncoder().encode(targets) else { return }
        SecItemDelete(base as CFDictionary)
        var add = base
        add[kSecValueData as String] = data
        // Same accessibility choice as the master key: usable from a cold
        // launch once the phone has been unlocked, never restored to other
        // hardware (the token only pairs with a PC this device could reach).
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(add as CFDictionary, nil)
        if status != errSecSuccess {
            NSLog("VellumShell: saving remote servers failed: \(status)")
        }
    }
}
