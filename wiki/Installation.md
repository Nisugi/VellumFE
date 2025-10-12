# Installation Guide

This guide will walk you through installing vellum-fe from source.

## Prerequisites

### 1. Rust Toolchain

vellum-fe requires Rust 1.70 or newer.

**Install Rust:**
```bash
# Linux/Mac
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Windows
# Download and run: https://rustup.rs/
```

**Verify installation:**
```bash
rustc --version
cargo --version
```

### 2. Lich Scripting Engine

You need Lich to connect to GemStone IV. vellum-fe connects to Lich via its detached mode.

**Install Lich:**
- Follow the [Lich installation guide](https://github.com/elanthia-online/lich-5)

**Windows users:** Ensure Ruby and Lich are properly installed and in your PATH.

### 3. Terminal Emulator

For the best experience, use a terminal with mouse support:

**Recommended terminals:**
- **Windows:** [Windows Terminal](https://aka.ms/terminal) (best)
- **Mac:** [iTerm2](https://iterm2.com/)
- **Linux:** Alacritty, Kitty, or GNOME Terminal

**Note:** Windows CMD and PowerShell 5.x have limited mouse support. Windows Terminal is strongly recommended.

## Building from Source

### 1. Clone the Repository

```bash
git clone https://github.com/yourusername/vellum-fe.git
cd vellum-fe
```

### 2. Build the Project

**Development build (faster compile, slower runtime):**
```bash
cargo build
```

Binary location: `target/debug/vellum-fe`

**Release build (slower compile, optimized runtime):**
```bash
cargo build --release
```

Binary location: `target/release/vellum-fe`

**Note:** Always use `--release` for actual gameplay. The debug build is significantly slower.

### 3. Run the Binary

**From the project directory:**
```bash
# Development
cargo run

# Release
cargo run --release

# Or run the binary directly
./target/release/vellum-fe
```

**From anywhere (optional):**

You can copy the binary to a directory in your PATH:

```bash
# Linux/Mac
sudo cp target/release/vellum-fe /usr/local/bin/

# Windows (PowerShell as Administrator)
Copy-Item target\release\vellum-fe.exe C:\Windows\System32\
```

## First Launch

On first launch, vellum-fe will:
1. Create `~/.vellum-fe/` directory
2. Generate a default `config.toml`
3. Create an `autosave` layout
4. Start logging to `debug.log` (if `RUST_LOG=debug` is set)

**Default config location:**
- **Linux/Mac:** `~/.vellum-fe/config.toml`
- **Windows:** `C:\Users\YourName\.vellum-fe\config.toml`

## Enabling Debug Logs

For troubleshooting, enable debug logging:

```bash
# Linux/Mac
RUST_LOG=debug cargo run --release

# Windows (PowerShell)
$env:RUST_LOG="debug"
cargo run --release

# Windows (CMD)
set RUST_LOG=debug
cargo run --release
```

Logs are written to `~/.vellum-fe/debug.log`.

## Platform-Specific Notes

### Windows

- Use **Windows Terminal** for best results
- PowerShell 7+ recommended over PowerShell 5.x
- If you see "VCRUNTIME140.dll not found", install [Visual C++ Redistributable](https://aka.ms/vs/17/release/vc_redist.x64.exe)

### Linux

- Ensure your terminal supports 24-bit color: `echo $COLORTERM` should show `truecolor`
- If mouse support doesn't work, try a different terminal emulator

### Mac

- iTerm2 has excellent mouse and color support
- Terminal.app works but has limited mouse features

## Updating vellum-fe

To update to the latest version:

```bash
cd vellum-fe
git pull origin main
cargo build --release
```

Your config and layouts in `~/.vellum-fe/` are preserved across updates.

## Uninstalling

To remove vellum-fe:

1. Delete the project directory
2. (Optional) Remove your config: `rm -rf ~/.vellum-fe`
3. (Optional) Remove binary from PATH if you copied it there

## Next Steps

- **[Quick Start Guide](https://github.com/Nisugi/VellumFE/wiki/Quick-Start)** - Launch and connect to Lich
- **[Window Management](https://github.com/Nisugi/VellumFE/wiki/Window-Management)** - Create your first windows
- **[Configuration Guide](https://github.com/Nisugi/VellumFE/wiki/Configuration-Guide)** - Customize your setup

---

← [Home](https://github.com/Nisugi/VellumFE/wiki/Home) | [Quick Start](https://github.com/Nisugi/VellumFE/wiki/Quick-Start) →
