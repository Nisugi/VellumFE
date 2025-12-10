# Installation

This guide covers installing VellumFE on all supported platforms.

## Quick Install

### Pre-built Binaries

Download the latest release for your platform from [GitHub Releases](https://github.com/nisugi/vellum-fe/releases):

| Platform | Download |
|----------|----------|
| Windows (x64) | `vellum-fe-windows-x64.zip` |
| Linux (x64) | `vellum-fe-linux-x64.tar.gz` |
| macOS (Intel) | `vellum-fe-macos-x64.tar.gz` |
| macOS (Apple Silicon) | `vellum-fe-macos-arm64.tar.gz` |

### Extract and Run

**Windows:**
```powershell
# Extract to a folder
Expand-Archive vellum-fe-windows-x64.zip -DestinationPath C:\vellum-fe

# Run
C:\vellum-fe\vellum-fe.exe
```

**Linux/macOS:**
```bash
# Extract
tar xzf vellum-fe-linux-x64.tar.gz

# Make executable
chmod +x vellum-fe

# Run
./vellum-fe
```

---

## Building from Source

For the latest features or unsupported platforms, build from source.

### Prerequisites

1. **Rust Toolchain** (1.70+)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Platform-Specific Dependencies**

   **Windows:**
   - Visual Studio Build Tools
   - vcpkg with OpenSSL:
     ```powershell
     git clone https://github.com/microsoft/vcpkg.git C:\vcpkg
     cd C:\vcpkg
     .\bootstrap-vcpkg.bat
     .\vcpkg install openssl:x64-windows
     $env:VCPKG_ROOT = "C:\vcpkg"
     ```

   **Linux (Debian/Ubuntu):**
   ```bash
   sudo apt install build-essential pkg-config libssl-dev libasound2-dev
   ```

   **Linux (Fedora/RHEL):**
   ```bash
   sudo dnf install gcc pkg-config openssl-devel alsa-lib-devel
   ```

   **macOS:**
   ```bash
   xcode-select --install
   # OpenSSL usually provided by system
   ```

### Build Steps

```bash
# Clone repository
git clone https://github.com/nisugi/vellum-fe.git
cd vellum-fe

# Build release version
cargo build --release

# Binary is at target/release/vellum-fe
```

### Build Options

```bash
# Build without sound support (smaller binary)
cargo build --release --no-default-features

# Build with all features
cargo build --release --features sound
```

---

## Data Directory

VellumFE stores configuration in a data directory:

| Platform | Default Location |
|----------|------------------|
| Windows | `%USERPROFILE%\.vellum-fe\` |
| Linux | `~/.vellum-fe/` |
| macOS | `~/.vellum-fe/` |

### Structure

```
~/.vellum-fe/
├── config.toml          # Main configuration
├── layout.toml          # Window layout
├── keybinds.toml        # Key bindings
├── highlights.toml      # Text highlighting
├── colors.toml          # Color theme
├── simu.pem             # eAccess certificate (auto-downloaded)
├── vellum-fe.log         # Debug log
└── profiles/            # Per-character profiles
    └── CharName/
        ├── layout.toml
        └── highlights.toml
```

### Custom Data Directory

Override the default location:

```bash
# Via command line
vellum-fe --data-dir /path/to/custom/dir

# Via environment variable
export VELLUM_FE_DIR=/path/to/custom/dir
vellum-fe
```

---

## Verifying Installation

1. **Check version:**
   ```bash
   vellum-fe --version
   ```

2. **View help:**
   ```bash
   vellum-fe --help
   ```

3. **Test terminal colors:**
   Launch VellumFE and check if colors display correctly. If colors look wrong:
   ```bash
   # Set truecolor support
   export COLORTERM=truecolor
   vellum-fe
   ```

---

## Troubleshooting Installation

### Windows: "vcruntime140.dll not found"
Install the [Visual C++ Redistributable](https://aka.ms/vs/17/release/vc_redist.x64.exe).

### Windows: OpenSSL errors
Ensure `VCPKG_ROOT` is set and OpenSSL is installed via vcpkg.

### Linux: "libasound.so not found"
Install ALSA library:
```bash
# Debian/Ubuntu
sudo apt install libasound2

# Fedora
sudo dnf install alsa-lib
```

### macOS: "cannot be opened because the developer cannot be verified"
Right-click the binary, select "Open", then click "Open" in the dialog.

### All Platforms: Colors not working
1. Use a modern terminal (Windows Terminal, iTerm2, etc.)
2. Set `COLORTERM=truecolor` environment variable
3. Ensure your terminal supports 24-bit color

---

## Next Steps

With VellumFE installed, continue to [First Launch](./first-launch.md) to connect to the game.

---

## See Also

- [Building from Source](../development/building.md) - Detailed build guide for developers
- [Configuration Reference](../configuration/README.md) - Config file documentation
- [Troubleshooting](../troubleshooting/README.md) - Common issues and solutions
