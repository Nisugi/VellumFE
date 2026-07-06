# Mobile Web

An optional embedded web server that lets your **phone's browser join the
running session**. It's a sidecar, not a separate mode — the TUI or GUI
keeps running on your PC, and the phone becomes a second screen and
controller for the same character.

> The web frontend is under active development; details here may evolve.

## Enabling

In `config.toml`:

```toml
[web]
enabled = true
port = 8040
bind = "0.0.0.0"   # required for phones on your LAN (default 127.0.0.1)
```

Or one-off from the CLI: `--web-port 8040` (this enables the server but
does **not** change `bind` — set that in config to reach it from a phone).

## Connecting from Your Phone

1. Find your PC's LAN address (e.g. `192.168.1.50`).
2. Open `http://192.168.1.50:8040/` in the phone browser.
3. Optional: "Add to Home Screen" — the client is a PWA and installs like
   an app.

> **Security**: there is **no authentication yet** — anyone who can reach
> the port controls your character. Keep the default `127.0.0.1` bind
> unless you're on a trusted LAN, and use Tailscale/WireGuard for off-LAN
> play. Never port-forward it to the internet.

## What You Can Do from the Phone

- **Read the game** live, with streams as filter chips (unread badges;
  long-press a chip to reorder — remembered per device).
- **Send commands** from the input bar — identical to typing at the PC,
  including dot-commands and command history shared both ways.
- **Tap links and nouns** — context menus open in a bottom sheet; tap
  exits in the room bar to move.
- **Vitals, hands, RT/CT** — a status strip with live countdowns.
- **Macro buttons** — the macro rail, menu buttons, and draggable floating
  buttons defined in [macros.toml](../configuration/macros-toml.md).
- **Create macros on the phone** — the rail's **+** button; phone-created
  buttons are saved server-side (`macros-local.toml`) and survive
  restarts. Hand-written buttons are read-only from the phone.
- **Reconnect gracefully** — on reconnect the client resumes where it left
  off, with a "missed output" marker if the gap was long. Multiple phones
  can connect at once.

## Tips

- After editing `macros.toml` on the PC, run `.reloadmacros` — connected
  phones update instantly.
- The wake-lock toggle in the phone UI keeps the screen on while hunting.
