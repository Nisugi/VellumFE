# The Launcher

The easiest way to start VellumFE: run it with **no arguments** (or just
double-click `vellum-fe.exe`) and a graphical launcher opens with your
saved connection profiles. One click launches a session; launch several
profiles to play multiple characters at once.

You can also open it explicitly with `vellum-fe --launcher`.

## Creating a Profile

Click **Add profile** and fill in:

- **Connection mode** — *Direct* (eAccess, no Lich) or *Lich* (proxy
  host/port)
- **Account / game / character** for direct mode, or **host / port** for
  Lich mode
- **Frontend** — GUI (default) or Terminal
- **Advanced options** — everything the CLI offers: web/phone port,
  sound off, settings profile, data directory, color mode, palette setup

## Passwords Go in the Keyring

Check **Save password** and the password is stored in your operating
system's secure credential store (Windows Credential Manager, macOS
Keychain, or the Linux secret service) — it is **never written to a
file** and never appears on a command line. Passwords are keyed by
account, so several profiles on the same account share one saved
password.

If you don't save it, the launcher prompts when you hit Launch (terminal
sessions ask in their own console instead).

## How Sessions Launch

Each **Launch** spawns a separate `vellum-fe --launch-profile NAME`
process — exactly the same startup path as a hand-typed command line, so
everything in this book applies unchanged. Terminal sessions open in
their own console window, which remembers its size and position across
launches.

## Skipping the Launcher

Saved profiles work from the command line or a shortcut too:

```bash
vellum-fe --launch-profile "Rolfard hunting"
```

Profile definitions live in `~/.vellum-fe/launcher.toml` (passwords
excluded — those stay in the keyring).
