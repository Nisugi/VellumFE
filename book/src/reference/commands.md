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
| `.reload [what]` | | Reload config from disk: `highlights`, `keybinds`, `settings`, `colors`, `layout`, or everything |

## Windows & Layout

| Command | Aliases | Description |
|---------|---------|-------------|
| `.windows` | | List all windows |
| `.addwindow [name type x y w [h]]` | | Add a window (no args opens a picker) |
| `.deletewindow <name>` | `.delwindow` | Delete a window |
| `.editwindow [name]` | `.editwin` | Edit a window (no name opens a picker) |
| `.hidewindow [name]` | `.hidewin` | Hide a window |
| `.rename <window> <new title>` | | Rename a window's title |
| `.border <window> <style> [color]` | | Set border sides: `all`, `none`, `top`, `bottom`, `left`, `right` |
| `.lockwindows` | `.lockall`, `.unlockwindows`, `.unlockall` | Toggle move/resize lock on all windows |
| `.savelayout [name]` | | Save the current layout |
| `.loadlayout [name]` | | Load a saved layout |
| `.layouts` | | List saved layouts |
| `.resize` | | Refit layout to the current terminal size |
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
| `.keybinds` | `.kb` | Browse keybinds |
| `.addkeybind` | `.addkey` | Create a keybind |
| `.savekeybinds [name]` | `.savekb` | Save keybinds as a named profile |
| `.loadkeybinds <name>` | `.loadkb` | Load a keybind profile |
| `.keybindprofiles` | `.kbprofiles` | List keybind profiles |

## Colors & Themes

| Command | Aliases | Description |
|---------|---------|-------------|
| `.themes` | | Browse and apply themes |
| `.settheme <name>` | `.theme` | Switch theme by name |
| `.edittheme` | | Edit the current theme |
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
