# Mobile Web

An optional embedded web server that lets your **phone's browser join the
running session**. It's a sidecar, not a separate mode — the TUI or GUI
keeps running on your PC, and the phone becomes a second screen and
controller for the same character.

## Enabling

In `config.toml`:

```toml
[web]
enabled = true
port = 8040
bind = "0.0.0.0"   # required for phones on your LAN (default 127.0.0.1)
pinned = false     # see "Multiple characters" below
```

Or one-off from the CLI: `--web-port 8040` (this enables the server but
does **not** change `bind` — set that in config to reach it from a phone).

## Pairing Your Phone

Access requires a pairing token. In-game:

```
.webinfo
```

This prints the session URL (with the token included) and opens a QR code
in your browser — scan it with the phone. One pairing covers all your
characters; the phone remembers the token, so this is a one-time step per
device. Unpaired connections are refused, and repeated bad attempts are
locked out temporarily.

> **Security**: pairing keeps strangers out, but the traffic is plain
> HTTP on your LAN. For off-LAN play use Tailscale/WireGuard — never
> expose the port to the open internet.

## The Dashboard

- `/` is a **dashboard**: one card per running character session, health-
  checked and auto-refreshing — tap a card to play it.
- `/play` is the game client itself.
- "Add to Home Screen" works — the client is a PWA and installs like an app.

## Multiple Characters

Run several VellumFE instances and they share the web setup automatically:
each instance walks upward from the base port to find a free one, and all
of them appear on the dashboard. If you want a specific character to have
a **stable port** (for a direct `/play` bookmark), set `pinned = true` in
that character's profile config — it then binds exactly that port or
disables web for the session with a loud warning, never a silent neighbor
port.

## What You Can Do from the Phone

- **Read the game** live, with streams as filter chips (unread badges;
  long-press a chip to reorder — remembered per device).
- **Send commands** from the input bar — identical to typing at the PC,
  including dot-commands and command history shared both ways.
- **Tap links and nouns** — context menus open in a bottom sheet; tap
  exits in the room bar to move.
- **Vitals, hands, RT/CT** — a status strip with live countdowns.
- **Active effects** — buffs/debuffs as pills on phones, a sidebar on
  tablets.
- **Adjust text size** — the **Aa** control resizes story text, remembered
  per device.
- **Macro buttons** — the macro rail, menu buttons, and draggable floating
  buttons defined in [macros.toml](../configuration/macros-toml.md).
- **Create and edit macros on the phone** — the rail's **+** button;
  phone-created buttons are saved server-side (`macros-local.toml`) and
  survive restarts. Hand-written buttons are read-only from the phone.
- **Reconnect gracefully** — on reconnect the client resumes where it left
  off, with a "missed output" marker if the gap was long. Multiple phones
  can connect at once.

## Tips

- After editing `macros.toml` on the PC, run `.reloadmacros` — connected
  phones update instantly.
- The wake-lock toggle in the phone UI keeps the screen on while hunting.
- If the phone can't connect at all, check `bind` — the default
  `127.0.0.1` is reachable only from the PC itself; `.webinfo` warns
  about this.
