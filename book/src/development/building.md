# Building from Source

Complete instructions for building VellumFE on all supported platforms.

## Prerequisites

### Rust Toolchain

Install Rust via rustup:

```bash
# Install rustup (Linux/macOS)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Or on Windows, download from https://rustup.rs/
```

Verify installation:

```bash
rustc --version  # Should be 1.70+
cargo --version
```

### Platform-Specific Dependencies

#### Windows

**Option 1: Visual Studio Build Tools**

1. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/)
2. Select "Desktop development with C++"

**Option 2: vcpkg for OpenSSL**

```bash
# Clone vcpkg
git clone https://github.com/Microsoft/vcpkg.git
cd vcpkg
./bootstrap-vcpkg.bat

# Install OpenSSL
./vcpkg install openssl:x64-windows

# Set environment variable
set VCPKG_ROOT=C:\path\to\vcpkg
```

#### macOS

```bash
# Install Xcode Command Line Tools
xcode-select --install

# Install OpenSSL via Homebrew
brew install openssl

# Set OpenSSL path (add to ~/.zshrc or ~/.bashrc)
export OPENSSL_DIR=$(brew --prefix openssl)
```

#### Linux (Debian/Ubuntu)

```bash
sudo apt update
sudo apt install build-essential pkg-config libssl-dev
```

#### Linux (Fedora/RHEL)

```bash
sudo dnf install gcc openssl-devel pkg-config
```

#### Linux (Arch)

```bash
sudo pacman -S base-devel openssl pkg-config
```

## Basic Build

### Clone Repository

```bash
git clone https://github.com/your-repo/vellum-fe.git
cd vellum-fe
```

### Debug Build

For development with debug symbols:

```bash
cargo build
```

Binary location: `target/debug/vellum-fe`

### Release Build

For optimized production build:

```bash
cargo build --release
```

Binary location: `target/release/vellum-fe`

### Run Directly

Without building separately:

```bash
# Debug mode
cargo run -- --help

# Release mode
cargo run --release -- --host 127.0.0.1 --port 8000
```

## Build Profiles

### Debug Profile (default)

```toml
# Cargo.toml [profile.dev]
opt-level = 0
debug = true
```

Fast compilation, slow execution, large binary.

### Release Profile

```toml
# Cargo.toml [profile.release]
opt-level = 3
lto = true
codegen-units = 1
```

Slow compilation, fast execution, small binary.

### Custom Profile

Create a custom profile in `Cargo.toml`:

```toml
[profile.release-with-debug]
inherits = "release"
debug = true
```

Build with: `cargo build --profile release-with-debug`

## Feature Flags

VellumFE may have optional features:

```bash
# Build with specific features
cargo build --features "feature1,feature2"

# Build with all features
cargo build --all-features

# Build without default features
cargo build --no-default-features
```

Check `Cargo.toml` for available features.

## Cross-Compilation

### Setup

Install cross-compilation targets:

```bash
# List available targets
rustup target list

# Add a target
rustup target add x86_64-unknown-linux-gnu
rustup target add x86_64-pc-windows-gnu
rustup target add aarch64-apple-darwin
```

### Build for Target

```bash
cargo build --release --target x86_64-unknown-linux-gnu
```

### Using cross

For easier cross-compilation with Docker:

```bash
# Install cross
cargo install cross

# Build for Linux from any platform
cross build --release --target x86_64-unknown-linux-gnu
```

## Build Optimization

### Link-Time Optimization (LTO)

Already enabled in release profile:

```toml
[profile.release]
lto = true
```

### Strip Symbols

Reduce binary size:

```bash
# After building
strip target/release/vellum-fe
```

Or configure in `Cargo.toml`:

```toml
[profile.release]
strip = true
```

### Size Optimization

For minimal binary size:

```toml
[profile.release]
opt-level = "z"  # Optimize for size
lto = true
strip = true
panic = "abort"
codegen-units = 1
```

## Dependency Management

### Update Dependencies

```bash
# Check for outdated dependencies
cargo outdated

# Update all dependencies
cargo update

# Update specific dependency
cargo update -p package_name
```

### Audit Dependencies

```bash
# Install cargo-audit
cargo install cargo-audit

# Check for security vulnerabilities
cargo audit
```

## Build Troubleshooting

### OpenSSL Not Found

**Error**: `Could not find directory of OpenSSL installation`

**Windows Solution**:
```bash
# Set vcpkg root
set VCPKG_ROOT=C:\path\to\vcpkg

# Or set OpenSSL dir directly
set OPENSSL_DIR=C:\path\to\openssl
```

**macOS Solution**:
```bash
export OPENSSL_DIR=$(brew --prefix openssl)
export OPENSSL_INCLUDE_DIR=$OPENSSL_DIR/include
export OPENSSL_LIB_DIR=$OPENSSL_DIR/lib
```

**Linux Solution**:
```bash
# Debian/Ubuntu
sudo apt install libssl-dev pkg-config

# Ensure pkg-config can find it
pkg-config --libs openssl
```

### Linker Errors

**Error**: `linking with cc failed`

**Solutions**:
1. Install build tools for your platform
2. Check library paths are correct
3. Ensure all dependencies are installed

### Out of Memory

**Error**: `out of memory during compilation`

**Solutions**:
1. Use fewer codegen units:
   ```toml
   [profile.release]
   codegen-units = 1
   ```
2. Reduce parallel jobs:
   ```bash
   cargo build -j 2
   ```
3. Close other applications

### Slow Builds

**Improve build times**:

1. Use `sccache`:
   ```bash
   cargo install sccache
   export RUSTC_WRAPPER=sccache
   ```

2. Use `mold` linker (Linux):
   ```bash
   # Install mold
   sudo apt install mold

   # Configure in .cargo/config.toml
   [target.x86_64-unknown-linux-gnu]
   linker = "clang"
   rustflags = ["-C", "link-arg=-fuse-ld=mold"]
   ```

3. Incremental compilation (default in debug)

## CI/CD

### GitHub Actions Example

```yaml
name: Build

on: [push, pull_request]

jobs:
  build:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]

    steps:
      - uses: actions/checkout@v3

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Build
        run: cargo build --release

      - name: Test
        run: cargo test
```

## Verification

### Run Tests

```bash
cargo test
```

### Run Application

```bash
# Show help
./target/release/vellum-fe --help

# Test connection
./target/release/vellum-fe --host 127.0.0.1 --port 8000
```

### Check Binary

```bash
# File type
file target/release/vellum-fe

# Dependencies (Linux)
ldd target/release/vellum-fe

# Size
ls -lh target/release/vellum-fe
```

## See Also

- [Project Structure](./project-structure.md) - Codebase overview
- [Testing](./testing.md) - Test patterns
- [Contributing](./contributing.md) - Contribution guide

