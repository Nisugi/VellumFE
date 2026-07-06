# Frontends

VellumFE has one core and three ways to use it. All frontends share the
same configuration, highlights, keybinds, themes, and dot-commands.

| Frontend | How | Best for |
|----------|-----|----------|
| [Terminal (TUI)](./tui.md) | default | Playing in a terminal; SSH; lowest footprint |
| [Desktop GUI](./gui.md) | `--frontend gui` | Native windows, mouse-first layout editing, system fonts |
| [Mobile Web](./web.md) | `[web]` config or `--web-port` | Your phone joins the *same session* as a second screen/controller |

The TUI and GUI are alternatives — you run one or the other. The mobile web
frontend is a **sidecar**: it runs alongside whichever desktop frontend is
active, and both control the same character at the same time.
