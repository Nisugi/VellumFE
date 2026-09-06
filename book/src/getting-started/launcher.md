# The Launcher

> Double-click, choose GUI, Terminal, or Vellum Despana, then click
> **Launch** — with
> your password in the OS credential store instead of your shell history.

## What it's for

You play more than one character, and you would rather not retype an eAccess
account, a game world, and a port every time. The Launcher keeps each of those
as a named connection you click once. Passwords go into Windows Credential
Manager, the macOS Keychain, or the Linux secret service — never into a file,
never onto a command line where every process on the machine can read it. Each
**Launch** starts a separate session process, so running four characters at once
is four clicks. The Launcher is a normal native window, not a system-tray
application; running the binary with no arguments opens it whenever you need it.

<figure class="shot" data-shot="gui/launcher-profiles">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The <b>VellumFE Launcher</b> window: the heading <b>VellumFE</b>,
  the line <b>Choose a connection to launch</b>, and two saved connections, each
  with <b>Launch</b>, <b>Edit</b> and <b>Delete</b> buttons.</figcaption>
</figure>

## Set it up

The Launcher is a desktop window, so this is a frontend split: it belongs to
the desktop GUI, and the terminal and mobile tabs say what you use instead.

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Start `vellum-fe` with **no arguments** — double-clicking does exactly this —
   or run `vellum-fe --launcher` if you want it from a terminal.
2. Click **➕ New connection**. The form is headed **New connection**.
3. Fill in **Name** (this is also what `--launch-profile` takes), then choose
   **Connection**: **Direct** to reach the game through eAccess with no Lich, or
   **Lich** to attach to a detachable-client Lich you are already running.
   - **Direct** shows **Account**, **Password**, **Save password**, **Game**,
     and **Character**.
   - **Lich** shows **Host**, **Port**, and **Character**.
4. Tick **Save password** if you want it remembered. Leave it unticked and the
   Launcher asks each time.
5. Click **Save**.
6. Back in the list, click **Launch** on the row you just made.

→ **Expected result:** the row appears with your name in bold and a summary
beneath it — `Nisugi @ GemStone IV` for direct, or
`Nisugi via Lich @ 127.0.0.1:8000` for Lich. Clicking **Launch** opens the
selected presentation: a native window for GUI, a console for Terminal, or an
automatically paired browser tab for Vellum Despana. The status line at the
bottom reads **Launched &lt;name&gt;**.

While that session is active, its row says **Open** and reopens the existing
presentation instead of starting a duplicate. If the process is still alive
after its game connection has ended, the row says **Restart**: Vellum stops
that dormant runtime, waits for it to release its session registry entry, and
then performs a normal launch. **Stop** removes the dormant runtime without
relaunching it.

<figure class="shot" data-shot="gui/launcher-new-connection">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The <b>New connection</b> form in <b>Direct</b> mode, showing
  <b>Account</b>, the masked <b>Password</b> field with its 👁 reveal button,
  <b>Save password</b>, the <b>Game</b> dropdown and <b>Character</b>.</figcaption>
</figure>

{{#endtab}}
{{#tab name="Terminal (TUI)"}}

The Launcher itself is a graphical window — there is no text-mode version of the
connection list. What the terminal gets is the *result*: once you have saved a
connection, run it by name from any shell, and it starts in the terminal
frontend if that is what the connection says:

```bash
vellum-fe --launch-profile "Nisugi hunting"
```

The password is resolved from the OS credential store by the session itself, so
nothing sensitive appears in your shell history or in `ps`.

To create or edit connections you need one graphical run of
`vellum-fe --launcher`. If you never want the Launcher at all, connect with
flags instead — `vellum-fe --port 8000 --character Nisugi`, covered in
[First Launch](./first-launch.md).

> ⚠️ **A connection you launch from the terminal still obeys its own frontend
> setting** — one saved as **GUI** opens a window even when you start it from a
> shell. Change it under **Advanced** ▸ **Frontend**.

→ **Expected result:** a terminal session starts in the console you ran it from,
already logged in as that connection's character.
{{#endtab}}
{{#tab name="Mobile"}}

The Android **(in progress)** and iOS **(beta — via TestFlight)** apps don't read
`launcher.toml` — they keep their own connections on the device. What they give
you is the same set of choices in a different place:

- **play.net** and **Lich** tabs on the login screen, matching Direct and Lich
  above. The Lich tab's **custom launch command (optional)** field even
  cold-starts Lich over SSH, the phone's version of this page's SSH Launcher.
- The person icon opens **Characters**, the app's saved-server list, where
  **Scan QR to add** pairs the phone with a VellumFE session running on your PC.
  Run `.webinfo` there for the QR code.

That last one means the app can also be your second screen — you are not limited
to the browser for that. The two paths do different jobs: **the app is for
playing from anywhere**, with everything saved on the phone; **the browser is
for a screen beside your PC**, with nothing to install. The browser route is set
up under this Launcher's **Advanced** fold via **Web dashboard** and **Bind
address**. Both are covered in
[Put VellumFE on your phone](../how-to/vellum-on-your-phone.md).

→ **Expected result:** the phone app opens its own login screen with **play.net**
and **Lich** tabs; your desktop's saved connections are not listed there, because
the phone keeps its own.
{{#endtab}}
{{#endtabs}}

## Common setups

### One account, several characters, all at once

Make a connection per character, all pointing at the same **Account**:

1. **➕ New connection** → Name `Nisugi`, **Direct**, Account `MYACCT`,
   Password once, **Save password** ticked, Game **GemStone IV**, Character
   `Nisugi`. **Save**.
2. **➕ New connection** → Name `Alt`, same **Account** `MYACCT`, leave
   **Password** blank, **Save password** still ticked, Character `Alt`. **Save**.
3. Click **Launch** on both rows.

The saved password is keyed by account, not by connection, so the second one
reuses the first's stored password without you typing it again. Because each
**Launch** spawns its own process, both characters run side by side with
independent layouts.

→ Two session windows are open, each showing its own character, and the
Launcher's status line shows **Launched Alt** from the last click.

### A connection that starts in the terminal instead

The Launcher defaults everything to the desktop GUI. To make one connection open
the text interface:

1. **Edit** the connection.
2. Open the **Advanced** fold.
3. Set **Frontend** to **Terminal**.
4. While you are there, set **Color mode** to `direct` for true-color terminals,
   and tick **Palette** ▸ **Set up on startup** if you use `slot` mode on a
   256-color terminal. These two rows only appear when **Frontend** is
   **Terminal**.
5. **Save**, then **Launch**.

→ A console window titled **VellumFE** opens running the terminal frontend, and
it remembers its size and position the next time you launch that connection.

### A connection that opens in Vellum Despana

Vellum Despana presents Vellum's headless core and login system through a
desktop browser workspace. It does not create a second login or bypass the
saved connection:

1. **Edit** the saved connection you want to use. For Lich scripts, confirm it
   is a **Lich** connection with the expected host, detachable-client port, and
   character.
2. Open **Advanced** and set **Frontend** to **Vellum Despana**.
3. **Save**, then click **Launch** on that row.

Vellum applies the selected profile once, starts its WebUI, and opens the
authoritative paired `/despana` URL after the server has actually bound. There
is no second profile click in the browser and no token to copy. If the preferred
Web dashboard port is occupied and not pinned, the server may choose a nearby
port; the automatically opened URL already contains the correct port.

→ The default browser opens Vellum Despana already connecting to the saved
character. The native Launcher remains available for starting another
character. Closing the browser tab does not stop its independent session
process: enter `.exit` in Despana when you mean to close it completely (`.quit`
disconnects and returns Despana to its Launcher handoff). See
[Vellum Despana](../frontends/despana.md) for workspace, map, and
troubleshooting details.

### Serve this session to your phone

In the connection's **Advanced** fold:

1. Tick **Web dashboard** ▸ **Enable on port** and leave `8484`.
2. Set **Bind address** to `0.0.0.0` — the hint beside it reads
   **0.0.0.0 = allow LAN devices**. Leaving it at `127.0.0.1` restricts the
   server to the machine it runs on.
3. **Save**, **Launch**, then browse to `http://<your-pc-ip>:8484/play` on the
   phone.

→ The phone shows the live session, driven by the same core the desktop window
is rendering.

## Tips & gotchas

> ⚠️ **The Launcher defaults to the GUI; a hand-typed command line defaults to
> the TUI.** A new connection is created with **Frontend: GUI**, but `vellum-fe`
> invoked with flags uses `--frontend tui` unless you say otherwise. The same
> character launched two ways can land in different interfaces. Set GUI,
> Terminal, or Vellum Despana explicitly under **Advanced** ▸ **Frontend**. The CLI's
> `--frontend headless` runs a browser-only session directly; selecting Vellum Despana
> in the Launcher is the convenient saved-connection path to that desktop
> presentation.

> ⚠️ **Deleting a connection can delete the saved password with it.** The
> confirmation window **Delete profile?** says so: the keyring entry is removed
> *unless another connection still uses the same account*. Deleting your only
> `MYACCT` connection means the next one has to re-enter the password.

- **"Password was NOT stored"** in red means the credential store refused the
  write — common on headless Linux, inside WSL, or under a bare window manager
  with no secret service running. The connection still saves and still launches;
  you are asked for the password each time. Install a secret service (GNOME
  Keyring, KWallet) to fix it.
- **A red line about `launcher.toml`** on startup means the file could not be
  parsed. The Launcher deliberately shows the error rather than starting with an
  empty list, because saving from an empty list would overwrite your
  connections. Fix the file before saving anything.
- **Connection names cannot contain `"` or `%`.** The Launcher rejects them at
  save time, because a terminal session's name travels through a Windows `cmd`
  command line where neither can be passed safely.
- **Renaming a connection does not duplicate it.** The edit form tracks the name
  you started with and replaces that entry.
- **The account name is not shown in the list.** Each row's summary is
  `<character> @ <game>` on purpose — the list is on screen constantly, and in
  screenshots. The account stays inside the edit form.
- **Nothing in the list? "No saved connections yet"** with **Create one to get
  started** is the empty state, not an error.

### The other launcher: cold-starting Lich over SSH

There is a second, separate feature with a confusingly similar name. The
**SSH Launcher** does not manage connections — it starts a *headless Lich on
your home PC* from wherever you are, over an existing WireGuard or Tailscale
tunnel, then attaches to it. Use it when the machine that runs Lich is not the
machine you are sitting at.

It lives inside a running session, not in the Launcher window:

- Type `.launcher` (or bare `.launch`) in the command input to open the **SSH
  Launcher** panel.
- Fill in **Host (tunnel address)**, **User**, **SSH port**, **Remote OS**,
  optionally **Attach host**, and the **Launch command template** — where
  `{character}`, `{game}` and `{port}` are substituted per character.
- Click **Generate new key**, then **Copy public key** and paste that one line
  into `~/.ssh/authorized_keys` on the home PC. The private half goes to your OS
  secure store; the indicator changes to **✓ key stored**. Expand **Harden it
  (recommended)** to see the `restrict,command="…"` prefix that limits a leaked
  key to launching the game and nothing else.
- Add each character under **Characters** with its game token and its own
  detachable-client **Port**, then **Save**.
- Run it with `.launch <character>`.

If a Lich is already listening on that port, the flow skips the SSH step
entirely and attaches straight away. If not, it SSHes in, spawns Lich detached
so it survives the SSH channel closing, then polls the port. The open port — not
the spawn's exit code — is what counts as success, so a message like *"Launched
Lich but 100.64.0.5:8001 never opened"* means the command template or the
character name is wrong, and the spawner's own output is appended to tell you
which. On a first connection the host key's fingerprint is pinned; if a pinned
key ever *changes*, the launch is refused outright rather than prompting.

→ `.launch Nisugi` reports progress in the session and ends attached to the
freshly started Lich.

## See also

- [Installing VellumFE](./installation.md) — getting the binary in the first place
- [First Launch](./first-launch.md) — connecting with flags instead of connections
- [Desktop GUI](../frontends/gui.md) · [Terminal (TUI)](../frontends/tui.md) ·
  [Vellum Despana](../frontends/despana.md)
- [Put VellumFE on your phone](../how-to/vellum-on-your-phone.md) — the web dashboard

<details>
<summary>Config reference (TOML)</summary>

### `~/.vellum-fe/launcher.toml` — the Launcher's connections

Written by the Launcher as `[[profiles]]` entries. **No passwords are ever
written here.**

| Field | Type | Default | What it does |
|---|---|---|---|
| `name` | string | *(required)* | Display name and the key `--launch-profile` takes. Cannot contain `"` or `%`. |
| `mode` | `"direct"` \| `"lich"` | *(required)* | eAccess login, or attach to a running Lich. |
| `account` | string | `""` | play.net account (direct only). Keys the saved password. |
| `game` | string | `"prime"` | One of `prime`, `platinum`, `shattered`, `test`, `dr`, `drplatinum`, `drfallen`, `drtest`. |
| `password_saved` | bool | `false` | True when a password for `account` is in the OS credential store. |
| `character` | string | `""` | Character to log in as; also selects that character's settings and layout. |
| `frontend` | `"gui"` \| `"tui"` | `"gui"` | Native fallback surface. Despana deliberately stores `"gui"` here so older Vellum builds can still read and launch the profile. |
| `web_client` | `"despana"` | *unset* | Selects Vellum Despana as the browser presentation. This additive field is ignored by older Vellum builds. |
| `host` | string | `"127.0.0.1"` | Lich host (lich mode). |
| `port` | u16 | `8000` | Lich detachable-client port (lich mode). |
| `custom_launch` | string | *unset* | Full Lich launch line; when present, connecting probes the port and SSH-launches Lich if it is down. |
| `web_port` | u16 | *unset* | Enables the embedded web server on this port. |
| `web_bind` | string | *unset* (= `127.0.0.1`) | `0.0.0.0` lets other devices on your network connect. |
| `nosound` | bool | `false` | Skip audio device initialization entirely. |
| `settings_profile` | string | *unset* | Use this settings folder instead of the character name, so several characters can share one setup. |
| `data_dir` | string | *unset* (= `~/.vellum-fe`) | Per-connection override of the base directory. |
| `color_mode` | `"direct"` \| `"slot"` | *unset* | Terminal color rendering (terminal frontend only). |
| `setup_palette` | bool | `false` | Run `.setpalette` at startup (pairs with `slot`). |

Passwords are stored through the `keyring` crate under the service id
`vellum-fe`, keyed by the lowercased account name. A just-typed password handed
to a spawned session travels in the private `VELLUM_FE_PASSWORD` environment
variable, which the session consumes and removes immediately — never on a
command line.

### `~/.vellum-fe/ssh-launcher.toml` — the SSH Launcher

Written by the **SSH Launcher** panel. **No key material is ever written here.**

| Field | Type | Default | What it does |
|---|---|---|---|
| `ssh.host` | string | `""` | Tunnel address of the home PC. |
| `ssh.user` | string | `""` | SSH user on the home PC. |
| `ssh.port` | u16 | `22` | SSH port. |
| `ssh.remote_os` | `"windows"` \| `"unix"` | `"windows"` | Chooses the detach mechanism for the spawned process. |
| `ssh.lich_command` | string | `""` | Launch template. `{character}`, `{game}` and `{port}` are substituted; quoted paths are split correctly. |
| `ssh.attach_host` | string | `""` | Where to attach after launch. Empty falls back to `ssh.host`. |
| `ssh.key_saved` | bool | `false` | True when the ed25519 private key is in the OS secure store. |
| `characters.<Name>.game` | string | *(required)* | Game token substituted into `{game}` — `gemstone` for GS4. |
| `characters.<Name>.port` | u16 | *(required)* | That character's detachable-client port. Give each character its own. |

The private key is stored under the same `vellum-fe` keyring service with the
account prefix `ssh-launcher-key:`, so it can never collide with a play.net
password. Host keys are pinned on first use to
`~/.vellum-fe/ssh-launcher-known-hosts`, which is separate from your personal
`~/.ssh/known_hosts` — VellumFE never touches your own SSH state.

</details>
