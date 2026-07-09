import Foundation
import Security

/// The one remembered remote VellumFE server (host, port, pairing token)
/// for the Remote login tab, stored as a single Keychain item — same trust
/// posture as the master password key in `CryptoKeys.swift`: device-bound,
/// never in backups. Mirrors the Android shell's `RemoteStore.kt`.
enum RemoteStore {
    struct Target: Codable, Equatable {
        var host: String
        var port: Int
        /// Pairing token for that PC's web server; empty when the user
        /// paired without one (the remote page prompts instead).
        var token: String
    }

    private static let base: [String: Any] = [
        kSecClass as String: kSecClassGenericPassword,
        kSecAttrService as String: "dev.vellumfe.remote-server",
        kSecAttrAccount as String: "vellum-remote",
    ]

    static func load() -> Target? {
        var query = base
        query[kSecReturnData as String] = true
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
              let data = item as? Data,
              let target = try? JSONDecoder().decode(Target.self, from: data)
        else { return nil }
        return target
    }

    static func save(_ target: Target) {
        guard let data = try? JSONEncoder().encode(target) else { return }
        SecItemDelete(base as CFDictionary)
        var add = base
        add[kSecValueData as String] = data
        // Same accessibility choice as the master key: usable from a cold
        // launch once the phone has been unlocked, never restored to other
        // hardware (the token only pairs with a PC this device could reach).
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(add as CFDictionary, nil)
        if status != errSecSuccess {
            NSLog("VellumShell: saving remote server failed: \(status)")
        }
    }

    static func forget() {
        SecItemDelete(base as CFDictionary)
    }
}
