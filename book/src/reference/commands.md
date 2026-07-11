# Command Reference

Anything you type starting with `.` is handled by VellumFE instead of being
sent to the game. Command names are case-insensitive; `Tab` completes them.
`.help` prints this list in-game. Unknown commands print a hint.

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
| `.go2 <target>` | | Native map travel: room id, uid (`u7150105`), tag (`bank`), saved name, or text search — see the [Travel chapter](../widgets/travel.md) |
| `.go2 stop` / `.go2 status` | | Cancel / show the active trip |
| `.go2 save <name> [id]` | | Save a travel target (`.go2 targets` lists them, `.go2 back` returns to the trip start) |

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
| `.savelayout [name]` | | Save the current layout under a name (each frontend keeps its own: TUI `.toml` grids, GUI checkpoints) |
| `.loadlayout <name>` | | Load a saved layout; in the GUI it applies live to the running session |
| `.layouts` | | List saved layouts for this frontend |
| `.resize` | | Refit layout to the current terminal size (TUI) |
| `.nexttab` / `.prevtab` | | Switch tabs in a tabbed window |
| `.gonew` | `.nextunread` | Jump to the next tab with unread messages |

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

## Misc

| Command | Aliases | Description |
|---------|---------|-------------|
| `.transparent` | | Toggle transparent window backgrounds (TUI) |
| `.containers` | | Toggle container discovery (LOOK IN a container spawns a window for it) |
| `.hidecontainers [title]` | | Close container windows (all, or one by title) |
| `.reloadmacros` | | Reload macros.toml and push to connected phones |
| `.webinfo` | | Show the phone pairing URL / app link and open their QR codes |
| `.webui [page\|off]` | | Lich WebUI panels (GUI, Lich 5.18+): no args picks from Lich's registered pages, a name opens that page, `off` disconnects |
