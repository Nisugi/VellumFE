# Troubleshooting

## Connection

**Can't connect to Lich ("connection refused")**
1. Make sure Lich is running and logged in
2. Check the port Lich is listening on and match it: `--port 8000`

**Direct eAccess authentication fails**
1. Verify credentials (test them with another client first)
2. Delete the cached certificate and retry: remove `~/.vellum-fe/simu.pem`
3. Check `~/.vellum-fe/vellum-fe.log` for details

## Display

**Colors look wrong**
1. Use a true-color terminal with `color_mode = "direct"` (Windows
   Terminal, kitty, alacritty, WezTerm)
2. On 256-color terminals, set `color_mode = "indexed"`, or `"slot"` plus
   `.setpalette`
3. On Unix, check `TERM` is something like `xterm-256color`

**Text or borders garbled**
1. Ensure the terminal uses UTF-8 and a font with box-drawing glyphs
   (Nerd Fonts work well)
2. Try a different terminal emulator

**Layout looks broken after resizing the terminal**
Run `.resize` to refit the layout, or `.savelayout` a size that works.
`vellum-fe validate-layout` checks a layout file for errors.

## Input

**Backspace doesn't work**
Your terminal sends `delete` instead. In keybinds.toml `[user]`, change
`backspace = "cursor_backspace"` to `delete = "cursor_backspace"`.

**A keybind does nothing**
1. Check the key isn't captured by your terminal or OS
2. Run with `RUST_LOG=debug` and check the log for `KEY EVENT` lines to
   see what your terminal actually sends
3. Check for conflicts with `[app]`/`[menu]` bindings, which take priority

## Highlights

**Pattern doesn't match**
1. Patterns are regexes — escape literals: `\.` `\(` `\[`
2. Use `(?i)` for case-insensitive matching
3. Test live with `.testline some text that should match`

**No sound plays**
1. `[sound] enabled = true` and `[highlights] sounds_enabled = true` in
   config.toml
2. The file must exist in `~/.vellum-fe/global/sounds/`
3. Launch without `--nosound`

## Performance

**Slow or high CPU**
1. Convert big `|`-lists of literal words to `fast_parse = true`
2. Simplify complex regexes; anchor them (`^...`) where possible
3. Reduce `buffer_size` on text windows

## Startup

**Crash or config error at startup**
1. The error usually names the file and line — check TOML syntax there
2. Move the offending file aside to regenerate defaults
3. Check `~/.vellum-fe/vellum-fe.log`; run with `RUST_LOG=debug` for more

## Still Stuck?

Open an issue at
[github.com/Nisugi/VellumFE/issues](https://github.com/Nisugi/VellumFE/issues)
with your version (`vellum-fe --version`), OS/terminal, and the relevant
log lines from `~/.vellum-fe/vellum-fe.log`.
