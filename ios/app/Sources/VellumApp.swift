import SwiftUI
import UIKit

/// The shell is just glass over the embedded web frontend; the Rust core
/// runs on its own thread inside the process. There is no CoreService
/// analog — iOS has no foreground services. Backgrounding suspends the
/// whole process (Rust threads freeze, the game TCP goes stale); on resume
/// the headless reconnect supervisor and the web client's ws backoff
/// restore the session. The one lever we have is a ~30s background grace
/// window, enough for quick app switches to keep the session alive.
@main
struct VellumApp: App {
    @Environment(\.scenePhase) private var scenePhase
    @State private var graceTask: UIBackgroundTaskIdentifier = .invalid

    var body: some Scene {
        WindowGroup {
            ContentView()
        }
        .onChange(of: scenePhase) { phase in
            switch phase {
            case .background:
                beginGrace()
            case .active:
                endGrace()
            default:
                break
            }
        }
    }

    private func beginGrace() {
        endGrace()
        graceTask = UIApplication.shared.beginBackgroundTask(withName: "vellum-linger") {
            // Expiration: end the task promptly or iOS terminates the app.
            endGrace()
        }
    }

    private func endGrace() {
        guard graceTask != .invalid else { return }
        UIApplication.shared.endBackgroundTask(graceTask)
        graceTask = .invalid
    }
}
