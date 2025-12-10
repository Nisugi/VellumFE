# Development Guide

Resources for developers who want to build, modify, or contribute to VellumFE.

## Overview

VellumFE is written in Rust, targeting terminal-based user interfaces. This section covers:

- Building from source
- Understanding the codebase
- Extending functionality
- Contributing to the project

## Quick Start

### Clone and Build

```bash
# Clone the repository
git clone https://github.com/your-repo/vellum-fe.git
cd vellum-fe

# Build in release mode
cargo build --release

# Run
./target/release/vellum-fe --help
```

### Prerequisites

- Rust 1.70+ (latest stable recommended)
- Cargo (comes with Rust)
- OpenSSL development libraries
- Platform-specific build tools

## Documentation Structure

### [Building](./building.md)

Complete build instructions for all platforms:
- Development builds
- Release builds
- Cross-compilation
- Troubleshooting build issues

### [Project Structure](./project-structure.md)

Tour of the codebase:
- Directory layout
- Module organization
- Key files and their purposes
- Architecture overview

### [Adding Widgets](./adding-widgets.md)

Create new widget types:
- Widget trait implementation
- State management
- Rendering
- Testing widgets

### [Adding Browsers](./adding-browsers.md)

Create new popup browser windows:
- Browser trait implementation
- Integration with widget system
- Event handling

### [Parser Extensions](./parser-extensions.md)

Extend the XML parser:
- Adding new element types
- Custom parsing logic
- Stream handling

### [Testing](./testing.md)

Test patterns and practices:
- Unit tests
- Integration tests
- Manual testing
- CI/CD

### [Contributing](./contributing.md)

How to contribute:
- Code style
- Pull request process
- Issue reporting
- Community guidelines

## Development Workflow

### Typical Development Cycle

```
1. Create feature branch
   git checkout -b feature/my-feature

2. Make changes
   - Write code
   - Add tests
   - Update documentation

3. Test locally
   cargo test
   cargo run -- [test args]

4. Format and lint
   cargo fmt
   cargo clippy

5. Submit PR
   git push origin feature/my-feature
```

### Development Build vs Release

| Aspect | Debug | Release |
|--------|-------|---------|
| Command | `cargo build` | `cargo build --release` |
| Speed | Slow | Fast |
| Binary size | Large | Optimized |
| Debug symbols | Yes | No (by default) |
| Compile time | Fast | Slower |

Use debug builds for development, release for testing performance.

## Architecture Overview

### Three-Layer Design

```
┌───────────────────────────────────────────────┐
│                  Frontend (TUI)               │
│        Rendering, Input Handling, Themes      │
├───────────────────────────────────────────────┤
│                    Core                       │
│   State Management, Business Logic, Events    │
├───────────────────────────────────────────────┤
│                    Data                       │
│        Parsing, Models, Serialization         │
└───────────────────────────────────────────────┘
```

### Key Components

| Component | Purpose | Location |
|-----------|---------|----------|
| Parser | XML protocol handling | `src/parser.rs` |
| Widgets | UI components | `src/data/widget.rs` |
| State | Application state | `src/core/` |
| TUI | Terminal rendering | `src/frontend/tui/` |
| Config | Configuration loading | `src/config.rs` |
| Network | Connection handling | `src/network.rs` |

## Getting Help

### Documentation

- This guide (you're reading it)
- Inline code documentation (`cargo doc --open`)
- Architecture docs in `docs/`

### Community

- GitHub Issues for bugs and features
- Discussions for questions

### Code Questions

- Read existing similar code
- Check test files for usage examples
- Ask in GitHub Discussions

## Tools

### Recommended IDE

- VS Code with rust-analyzer
- IntelliJ IDEA with Rust plugin
- Vim/Neovim with rust.vim

### Useful Commands

```bash
# Generate documentation
cargo doc --open

# Run specific test
cargo test test_name

# Check without building
cargo check

# Format code
cargo fmt

# Lint code
cargo clippy

# Watch for changes
cargo watch -x check
```

## See Also

- [Architecture Overview](../architecture/README.md) - System design
- [Configuration](../configuration/README.md) - Config file format
- [Widget Reference](../widgets/README.md) - Widget documentation

