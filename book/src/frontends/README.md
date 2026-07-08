# Frontends

VellumFE is one core with several faces. All of them share the same
configuration, highlights, keybinds, themes, and dot-commands.

| Frontend | How | Best for |
|----------|-----|----------|
| [Terminal (TUI)](./tui.md) | default | Playing in a terminal; SSH; lowest footprint |
| [Desktop GUI](./gui.md) | `--frontend gui` | Native windows, mouse-first layout editing, graphics & skins |
| [Mobile Web](./web.md) | `[web]` config or `--web-port` | Your phone joins a *desktop session* as a second screen |
| [Headless](./web.md#headless-mode) | `--frontend headless` | No local UI — a browser is the whole interface |
| [Android App](./android.md) | sideloaded APK | The whole client on your phone, no PC at all |
| [iOS App](./ios.md) | TestFlight (beta) | Same as Android, for iPhone |

Three ways to think about it:

- **At the PC**: run the TUI or GUI.
- **PC hosts, phone joins**: run TUI/GUI with the web server enabled —
  the phone is a second controller for the same session.
- **No PC**: headless mode on any machine with a browser pointed at it,
  or the Android/iOS apps, which bundle the core and the web UI into one
  phone app.
