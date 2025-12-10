# Version History

Release notes, breaking changes, and upgrade instructions.

## Current Version

**Version**: 0.1.0 (Development)

## Changelog Format

Each release includes:
- **New Features**: New functionality
- **Improvements**: Enhancements to existing features
- **Bug Fixes**: Resolved issues
- **Breaking Changes**: Changes requiring user action
- **Known Issues**: Outstanding problems

---

## Version 0.1.0 (Planned)

Initial public release.

### New Features

- Core application framework
- Lich proxy connection mode
- Direct eAccess authentication
- Widget system with 15+ widget types
- TOML-based configuration
- Keybind and macro system
- Trigger automation
- Text highlighting
- Color themes
- TTS support

### Widget Types

- `text` - Scrollable text windows
- `tabbed_text` - Tabbed text windows
- `progress` - Vital bars
- `countdown` - Roundtime/casttime
- `compass` - Navigation compass
- `indicator` - Status indicators
- `room` - Room information
- `command_input` - Command entry
- `injury_doll` - Injury display
- `active_effects` - Active spells/effects
- `inventory` - Inventory view
- `spells` - Known spells
- `hand` - Hand contents
- `dashboard` - Composite status
- `performance` - Performance metrics

### Supported Platforms

- Windows 10/11
- macOS 12+
- Linux (glibc 2.31+)

### Known Issues

- Direct mode certificate handling edge cases
- Some Unicode rendering issues on older terminals

---

## Upgrading

### General Upgrade Process

1. **Backup configuration**:
   ```bash
   cp -r ~/.vellum-fe ~/.vellum-fe.backup
   ```

2. **Download new version**

3. **Replace binary**

4. **Review release notes** for breaking changes

5. **Update configuration** if needed

6. **Test** before deleting backup

### Checking Version

```bash
vellum-fe --version
```

### Configuration Compatibility

Configuration files are versioned. When format changes:

1. VellumFE will warn about outdated config
2. Auto-migration may be attempted
3. Manual migration instructions provided if needed

---

## Breaking Changes Guide

### Configuration Format Changes

If a configuration key is renamed or restructured:

**Before**:
```toml
[old_section]
old_key = "value"
```

**After**:
```toml
[new_section]
new_key = "value"
```

**Migration**: Update your config files manually or run:
```bash
vellum-fe --migrate-config
```

### Keybind Changes

If keybind syntax changes:

**Before**:
```toml
[keybinds.F1]
command = "look"
```

**After**:
```toml
[keybinds."f1"]
macro = "look"
```

### Widget Changes

If widget types or properties change:

**Before**:
```toml
[[widgets]]
type = "old_type"
old_property = "value"
```

**After**:
```toml
[[widgets]]
type = "new_type"
new_property = "value"
```

---

## Deprecation Policy

### Deprecation Timeline

1. **Announcement**: Feature marked deprecated in release notes
2. **Warning Period**: Deprecated features log warnings
3. **Removal**: Feature removed in future major version

### Deprecation Notices

Deprecated features are logged:
```
[WARN] 'old_feature' is deprecated, use 'new_feature' instead
```

---

## Development Versions

### Pre-release Tags

- `alpha` - Early testing, unstable
- `beta` - Feature complete, testing
- `rc` - Release candidate, final testing

Example: `0.2.0-beta.1`

### Building from Source

For development versions:

```bash
git clone https://github.com/vellum-fe/vellum-fe
cd vellum-fe
git checkout develop
cargo build --release
```

---

## Reporting Issues

### When Upgrading

If you encounter issues after upgrading:

1. Check release notes for breaking changes
2. Verify configuration is valid
3. Try with default configuration
4. Check [Troubleshooting](../troubleshooting/README.md)
5. Report on GitHub if unresolved

### Issue Template

```markdown
**Version**: [e.g., 0.1.0]
**Previous Version**: [if upgrade issue]
**Platform**: [Windows/macOS/Linux]

**Description**:
[What happened]

**Expected**:
[What should happen]

**Steps**:
1. [Step 1]
2. [Step 2]

**Configuration** (if relevant):
```toml
[relevant config]
```
```

---

## Future Roadmap

### Planned Features

- Advanced scripting integration
- Plugin system
- More widget types
- Enhanced accessibility
- Performance improvements

### Community Requests

Track feature requests on GitHub Issues.

---

## Version Support

### Support Matrix

| Version | Status | Support |
|---------|--------|---------|
| 0.1.x | Current | Full support |
| Pre-release | Development | Limited |

### End of Life

When a version reaches EOL:
- Security patches may cease
- No new features
- Upgrade recommended

---

## See Also

- [Installation](../getting-started/installation.md)
- [Configuration](../configuration/README.md)
- [Troubleshooting](../troubleshooting/README.md)

