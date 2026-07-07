import Foundation
import Security

/// Password-at-rest hardening: a 32-byte master key stored in the iOS
/// Keychain (device-bound, never in backups), handed to the Rust core via
/// VELLUM_PASSWORD_KEY. The core seals stored password values with it —
/// the passwords file on disk is unreadable without this device's Keychain.
///
/// Simpler than the Android analog (`CryptoKeys.kt`): the Keychain stores
/// small secrets directly, so there is no wrap-key-plus-file dance.
///
/// Best-effort: if the Keychain is unavailable the core falls back to its
/// previous behavior (app-private plaintext), never losing saved logins.
enum CryptoKeys {
    private static let service = "dev.vellumfe.master-key"
    private static let account = "vellum-master"
    private static var installed = false

    /// Idempotent; must run *before* `CoreBridge.startCore` so the env var
    /// is visible when the core first reads it.
    static func installPasswordKey() {
        guard !installed else { return }
        do {
            let master = try loadOrCreateMasterKey()
            let hex = master.map { String(format: "%02x", $0) }.joined()
            setenv("VELLUM_PASSWORD_KEY", hex, 1)
            installed = true
        } catch {
            NSLog("VellumShell: password key unavailable; passwords stored unencrypted: \(error)")
        }
    }

    private enum KeychainError: Error {
        case status(OSStatus)
        case randomFailed(OSStatus)
    }

    private static func loadOrCreateMasterKey() throws -> [UInt8] {
        let base: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]

        var query = base
        query[kSecReturnData as String] = true
        var item: CFTypeRef?
        let found = SecItemCopyMatching(query as CFDictionary, &item)
        if found == errSecSuccess, let data = item as? Data, data.count == 32 {
            return [UInt8](data)
        }
        // Missing or malformed: regenerate. (A wrong-sized entry would be
        // rejected by the core's exactly-32-bytes check anyway.)
        SecItemDelete(base as CFDictionary)

        var master = [UInt8](repeating: 0, count: 32)
        let rand = SecRandomCopyBytes(kSecRandomDefault, master.count, &master)
        guard rand == errSecSuccess else { throw KeychainError.randomFailed(rand) }

        var add = base
        add[kSecValueData as String] = Data(master)
        // AfterFirstUnlock: the core can start from a cold app launch while
        // the phone is unlocked-once; ThisDeviceOnly keeps the key out of
        // backups so sealed passwords can't follow a restore to new hardware
        // (matching Android's hardware-bound Keystore semantics).
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let added = SecItemAdd(add as CFDictionary, nil)
        guard added == errSecSuccess else { throw KeychainError.status(added) }
        return master
    }
}
