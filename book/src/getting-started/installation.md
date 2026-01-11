# Installation

## Download

Download the latest release from [GitHub Releases](https://github.com/Nisugi/VellumFE/releases).

| Platform | File |
|----------|------|
| Windows | `vellum-fe-windows.zip` |
| macOS | `vellum-fe-macos.tar.gz` |
| Linux | `vellum-fe-linux.tar.gz` |

Extract the archive and place `vellum-fe` (or `vellum-fe.exe`) somewhere in your PATH.

## Building from Source

Requires [Rust](https://rustup.rs/) 1.70+.

```bash
git clone https://github.com/Nisugi/VellumFE.git
cd VellumFE
cargo build --release
```

The binary will be at `target/release/vellum-fe`.

### Windows: OpenSSL for Direct Mode

Direct eAccess authentication requires OpenSSL. Install via [vcpkg](https://vcpkg.io/):

```powershell
vcpkg install openssl:x64-windows
set VCPKG_ROOT=C:\path\to\vcpkg
cargo build --release
```

## Verify Installation

```bash
vellum-fe --version
```

Should display the version number (e.g., `vellum-fe 0.2.0-beta.11`).

## Configuration Directory

On first run, VellumFE creates `~/.vellum-fe/` with default configuration files.

You can override this location with the `VELLUM_FE_DIR` environment variable:

```bash
export VELLUM_FE_DIR=/custom/path
```
