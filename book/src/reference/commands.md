# Command Reference

Anything you type starting with `.` is handled by VellumFE instead of being
sent to the game. Command names are case-insensitive; `Tab` completes them.
`.help` prints an abbreviated version of this list in-game. Unknown commands
print a hint.

Everything else you type goes to the game unchanged. (Typing the game
command `quit` also saves your settings on the way out.)

## General

| Command | Aliases | Description |
|---------|---------|-------------|
| `.help` | `.h`, `.?` | List all commands |
| `.version` | `.ver` | Show VellumFE version |
| `.quit` | `.q` | Exit VellumFE (saves settings) |
| `.menu` | | Open the main menu |
| `.settings` | | Open the settings editor |
| `.reload [what]` | | Reload config from disk: `highlights`, `keybinds`, `hotbars`, `settings`, `colors`, `layout`, or everything |
| `.room` | | Show how the current room resolved against the map database (stream ids, mapdb room/location, routable exits, tags) — for debugging the map and pathing |
| `.mapdb [download\|remove\|repo <r>]` | | Manage downloaded map data from any frontend (no args = status). On phones this is *the* way to fetch the map — there's no Settings > Map panel there |
| `.go2 <target>` | | Native map travel: room id, uid (`u7150105`), tag (`bank`), saved name, or text search — see the [Travel chapter](../widgets/travel.md) |
| `.go2 stop` / `.go2 status` | | Cancel / show the active trip |
| `.go2 save <name> [id]` | | Save a travel target (`.go2 targets` lists them, `.go2 back` returns to the trip start) |
| `.portal [n\|word]` | | Walk the room's non-compass exit (`go door`, `climb stair`, ...) from the map data (room objects as fallback). One candidate walks it; several open a picker menu (keyboard/pad navigable) — or pick by number or word. Controller d-pad left by default |
| `.foreach ... in <bag>; cmd; cmd` | | Run commands over matching container items (native `;foreach`). No commands = dry-run list. See [Inventory Tools](../features/inventory-tools.md) |
| `.sorter [on\|off]` | | Categorize "look in container" output by item type (native `;sorter`). No arg toggles |
| `.stop` | | Stop whatever automation is driving — a `.go2` trip or a `.foreach` run |
| `.data [status\|reload]` | | Inspect / refresh the shared item database used by `.foreach` and `.sorter` (Lich folder → local → bundled) |

## Windows & Layout

| Command | Aliases | Description |
|---------|---------|-------------|
| `.windows` | | List all windows |
| `.addwindow [name type x y w [h]]` | | Add a window (no args opens a picker) |
| `.deletewindow <name>` | `.delwindow` | Hide a window (kept in the layout; in the GUI, the window editor's Delete Window button removes it for real) |
| `.editwindow [name]` | `.editwin` | Edit a window (no name opens a picker) |
| `.hidewindow [name]` | `.hidewin` | Hide a window |
| `.rename <window> <new title>` | | Rename a window's title |
| `.border <window> <style> [color]` | | Set border sides: `all`, `none`, `top`, `bottom`, `left`, `right` |
| `.lockwindows` | `.lockall`, `.unlockwindows`, `.unlockall` | Toggle move/resize lock on all windows |
| `.performance [dump]` | `.perf` | Toggle the [performance monitor](./performance.md); `dump` writes a diagnostic report file |
| `.savelayout [name]` | | Save the current layout under a name in the shared `~/.vellum-fe/layouts/` folder, loadable by any character (each frontend has its own format: TUI `.toml` grids, GUI `.json` checkpoints) |
| `.loadlayout <name>` | | Load a saved layout; in the GUI it applies live to the running session |
| `.layouts` | | List saved layouts for this frontend |
| `.resize` | | Refit layout to the current terminal size (TUI) |
| `.nexttab` / `.prevtab` | | Switch tabs in a tabbed window |
| `.gonew` | `.nextunread` | Jump to the next tab with unread messages |
| `.streams` | | Open the stream routing editor: every known stream and where it goes (a window, `main`, or discard) |

## Sharing Your UI

| Command | Aliases | Description |
|---------|---------|-------------|
| `.uiexport <name> [parts...]` | | Bundle the files that make your UI into `~/.vellum-fe/exports/<name>.vellumpack` — a single shareable file. Parts: `layout` (TUI grid + the GUI's live arrangement when exported from the GUI), `highlights`, `keybinds`, `hotbars`, `colors`, `macros`, `skin` (the active skin's whole folder). Default: all. Connection settings and passwords are never included. |
| `.uiimport <name\|file>` | | Preview a pack: its parts, skin, and file count |
| `.uiimport <name\|file> apply` | | Install a pack: replaced files are backed up to `~/.vellum-fe/backups/`, everything hot-reloads, and layouts land as named checkpoints (`.loadlayout <packname>`). Skins extract and activate. Unknown or unsafe entries in a pack are skipped, never written. |

Post packs in the community Discord — favorites can become shipped
default layouts.

## Highlights

| Command | Aliases | Description |
|---------|---------|-------------|
| `.highlights` | `.hl` | Browse highlights |
| `.addhighlight` | `.addhl` | Create a highlight |
| `.edithighlight [name]` | `.edithl` | Edit a highlight |
| `.testline <text>` | | Inject a fake game line to test patterns |
| `.savehighlights [name]` | `.savehl` | Save highlights as a named profile |
| `.loadhighlights [name]` | `.loadhl` | Load a highlight profile |
| `.highlightprofiles` | `.hlprofiles` | List highlight profiles |

## Keybinds

| Command | Aliases | Description |
|---------|---------|-------------|
| `.keybinds` | `.kb` | Browse keybinds (press `f` to cycle the scope filter: all / global / character) |
| `.controller` | | Edit gamepad button bindings (GUI; see [Controllers](../frontends/gui.md#controllers)) |
| `.addkeybind` | `.addkey` | Create a keybind |
| `.savekeybinds [name]` | `.savekb` | Save keybinds as a named profile |
| `.loadkeybinds <name>` | `.loadkb` | Load a keybind profile |
| `.keybindprofiles` | `.kbprofiles` | List keybind profiles |

## Hotbars

| Command | Aliases | Description |
|---------|---------|-------------|
| `.hotbars` | `.hotbar` | Open the hotbar editor (bars of command buttons; see [Hotbars](../widgets/hotkeybar.md)) |

## Colors & Themes

| Command | Aliases | Description |
|---------|---------|-------------|
| `.themes` | | Browse and apply themes |
| `.settheme <name>` | `.theme` | Switch theme by name |
| `.edittheme` | | Edit the current theme |
| `.skins` | | List installed GUI skins |
| `.setskin <name>` | `.skin` | Apply a skin (`.setskin none` disables; GUI only) |
| `.makeskin <name>` | | Create a starter skin to edit |
| `.reloadskin` | | Force-reload the active skin (after editing images) |
| `.colors` | `.colorpalette` | Browse the color palette |
| `.addcolor` | `.createcolor` | Add a palette color |
| `.uicolors` | | Edit UI element colors |
| `.spellcolors` | | Edit spell-circle colors |
| `.addspellcolor` | `.newspellcolor` | Add a spell color entry |
| `.setpalette` | | Load palette into terminal slots (TUI, 256-color mode) |
| `.resetpalette` | | Reset the terminal palette (TUI) |

## Text-to-Speech

`.tts` with no subcommand shows status. Settings changes save immediately.
The GUI has the same controls in Settings > Speech; per-window speech is
the "speak new lines" checkbox in the window editor (`tts_speak` in
layout.toml).

| Command | Description |
|---------|-------------|
| `.tts on` / `.tts off` | Enable / disable text-to-speech |
| `.tts mute` | Toggle mute without turning TTS off |
| `.tts rate <0.5-3.0>` | Speech rate (1.0 = normal) |
| `.tts volume <0.0-1.0>` | Speech volume |
| `.tts voice <name\|default>` | Pick a voice by name, or return to the engine default |
| `.tts voices` | List available voices |
| `.tts test` | Speak a sample line |
| `.tts clear` | Clear the pending speech queue |
| `.tts status` | Show enabled/muted state, rate, volume, voice, and queue depth |

## Misc

| Command | Aliases | Description |
|---------|---------|-------------|
| `.transparent` | | Toggle transparent window backgrounds (TUI) |
| `.hidecontainers [title]` | | Close container windows (all, or one by title) |
| `.reloadmacros` | | Reload macros.toml and push to connected phones |
| `.webinfo` | | Show the phone pairing URL / app link and open their QR codes |
| `.webui [page\|off]` | | Lich WebUI panels (GUI, Lich 5.18+): no args picks from Lich's registered pages, a name opens that page, `off` disconnects |
