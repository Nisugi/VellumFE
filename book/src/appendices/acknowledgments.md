# Acknowledgments

Recognition of projects, people, and resources that made VellumFE possible.

## Inspiration

### Profanity

VellumFE draws significant inspiration from [Profanity](https://github.com/rspeicher/profanity), the pioneering terminal-based client for GemStone IV. Profanity demonstrated that a modern, efficient terminal interface was not only possible but could provide a superior gameplay experience.

### Lich

[Lich](https://lichproject.org/) has been instrumental to the GemStone IV community, providing scripting capabilities and serving as an excellent proxy for frontend development. VellumFE's Lich mode exists thanks to Lich's well-designed proxy interface.

### StormFront and Wizard

The official Simutronics clients (StormFront, Wizard Front End) established the standard for GemStone IV interfaces and defined the XML protocol that VellumFE parses.

## Open Source Libraries

VellumFE is built on excellent open source foundations:

### Core

- **[Rust](https://www.rust-lang.org/)** - The programming language providing safety and performance
- **[Tokio](https://tokio.rs/)** - Asynchronous runtime for non-blocking I/O
- **[Ratatui](https://ratatui.rs/)** - Terminal user interface library

### Parsing

- **[quick-xml](https://github.com/tafia/quick-xml)** - Fast XML parser
- **[regex](https://github.com/rust-lang/regex)** - Regular expression engine
- **[aho-corasick](https://github.com/BurntSushi/aho-corasick)** - Multi-pattern string matching

### Configuration

- **[toml](https://github.com/toml-rs/toml)** - TOML parser and serializer
- **[serde](https://serde.rs/)** - Serialization framework
- **[dirs](https://github.com/soc/dirs-rs)** - Platform-specific directories

### Network

- **[openssl](https://github.com/sfackler/rust-openssl)** - TLS/SSL implementation
- **[rustls](https://github.com/rustls/rustls)** - Pure Rust TLS (alternative)

### Audio

- **[cpal](https://github.com/RustAudio/cpal)** - Cross-platform audio
- **[tts](https://github.com/ndarilek/tts-rs)** - Text-to-speech bindings

## Documentation

This documentation is built with:

- **[mdBook](https://rust-lang.github.io/mdBook/)** - The book-from-markdown tool
- **GitHub Pages** - Hosting

## Community

### GemStone IV Players

The GemStone IV community has been invaluable for:

- Feature suggestions
- Bug reports
- Beta testing
- Protocol documentation
- General encouragement

### Contributors

Thank you to everyone who has contributed code, documentation, bug reports, or feedback. See the [Contributors](https://github.com/your-repo/contributors) page for a complete list.

## Simutronics

GemStone IV is developed and operated by [Simutronics Corporation](https://www.simutronics.com/). VellumFE is an unofficial third-party client developed by fans.

**Disclaimer**: VellumFE is not affiliated with, endorsed by, or connected to Simutronics Corporation. GemStone IV and all related trademarks are property of Simutronics.

## Standards and References

VellumFE implementation references:

- **[ANSI Escape Codes](https://en.wikipedia.org/wiki/ANSI_escape_code)** - Terminal control sequences
- **[VT100](https://vt100.net/)** - Terminal emulation reference
- **[Unicode](https://unicode.org/)** - Character encoding standards
- **[WCAG](https://www.w3.org/WAI/standards-guidelines/wcag/)** - Web Content Accessibility Guidelines

## Special Thanks

- The Rust community for excellent tooling and documentation
- Terminal emulator developers for maintaining standards
- Everyone who plays GemStone IV and keeps the community alive
- Open source maintainers everywhere

## License

VellumFE is open source software. See the [LICENSE](https://github.com/your-repo/LICENSE) file for details.

---

*"Standing on the shoulders of giants."*

## Contributing

Want to be acknowledged? [Contribute](../development/contributing.md) to VellumFE! All contributions, from code to documentation to bug reports, are valued.

## See Also

- [Contributing Guide](../development/contributing.md)
- [Building from Source](../development/building.md)
- [Project Structure](../development/project-structure.md)

