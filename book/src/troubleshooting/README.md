# Troubleshooting

Diagnosis and solutions for common VellumFE issues.

## Quick Diagnosis

### Issue Categories

| Symptom | Likely Cause | Go To |
|---------|--------------|-------|
| Won't start | Missing dependency, bad config | [Common Errors](./common-errors.md) |
| Can't connect | Network, firewall, credentials | [Connection Issues](./connection-issues.md) |
| Slow/laggy | Performance settings, system load | [Performance Issues](./performance-issues.md) |
| Colors wrong | Theme, terminal settings | [Display Issues](./display-issues.md) |
| Platform-specific | OS configuration | [Platform Issues](./platform-issues.md) |

## First Steps

Before diving into specific issues, try these general steps:

### 1. Check Logs

```bash
# View recent logs
tail -100 ~/.vellum-fe/vellum-fe.log

# Watch logs in real-time
tail -f ~/.vellum-fe/vellum-fe.log
```

### 2. Verify Configuration

```bash
# Validate config syntax
vellum-fe --check-config

# Start with defaults (bypass config issues)
vellum-fe --default-config
```

### 3. Update VellumFE

```bash
# Check current version
vellum-fe --version

# Download latest release from GitHub
```

### 4. Check Dependencies

```bash
# Linux - verify libraries
ldd $(which vellum-fe)

# macOS - check dylibs
otool -L $(which vellum-fe)
```

## Log Levels

Increase logging for diagnosis:

```toml
# config.toml
[logging]
level = "debug"  # trace, debug, info, warn, error
file = "~/.vellum-fe/vellum-fe.log"
```

Log level details:
- `error` - Only critical failures
- `warn` - Warnings and errors
- `info` - Normal operation (default)
- `debug` - Detailed operation
- `trace` - Very verbose (large files)

## Common Patterns

### Configuration Issues

**Symptom**: VellumFE exits immediately or shows "config error"

**Diagnosis**:
```bash
# Check for TOML syntax errors
vellum-fe --check-config 2>&1 | head -20
```

**Common causes**:
- Missing quotes around strings with special characters
- Unclosed brackets or braces
- Invalid key names
- Wrong data types

### Connection Issues

**Symptom**: "Connection refused" or timeout

**Diagnosis**:
```bash
# Test Lich connection
nc -zv 127.0.0.1 8000

# Test direct connection
nc -zv eaccess.play.net 7910
```

### Display Issues

**Symptom**: Missing colors, garbled text, wrong symbols

**Diagnosis**:
```bash
# Check terminal capabilities
echo $TERM
tput colors

# Test Unicode support
echo "╔═══╗ ← Should show box drawing"
```

## Getting Help

### Information to Collect

When reporting issues, include:

1. **Version**: `vellum-fe --version`
2. **Platform**: Windows/macOS/Linux version
3. **Terminal**: What terminal emulator
4. **Config**: Relevant config sections (redact credentials!)
5. **Logs**: Recent log entries
6. **Steps**: How to reproduce

### Report Template

```markdown
**Version**: 0.1.0
**Platform**: Ubuntu 22.04
**Terminal**: Alacritty 0.12

**Description**:
What happened

**Expected**:
What should happen

**Steps to Reproduce**:
1. Start VellumFE with...
2. Do this...
3. See error

**Relevant Config**:
```toml
[connection]
mode = "lich"
```

**Log Output**:
```
[ERROR] relevant log lines
```
```

### Support Channels

1. **GitHub Issues**: Bug reports and feature requests
2. **GitHub Discussions**: Questions and help
3. **Documentation**: Check relevant sections first

## Troubleshooting Guides

### By Issue Type

- [Common Errors](./common-errors.md) - Error messages and solutions
- [Platform Issues](./platform-issues.md) - OS-specific problems
- [Performance Issues](./performance-issues.md) - Speed and responsiveness
- [Display Issues](./display-issues.md) - Visual problems
- [Connection Issues](./connection-issues.md) - Network problems

### By Symptom

| I see... | Check |
|----------|-------|
| "Config error" | [Common Errors](./common-errors.md#configuration-errors) |
| "Connection refused" | [Connection Issues](./connection-issues.md#connection-refused) |
| "Certificate error" | [Connection Issues](./connection-issues.md#certificate-issues) |
| Garbled text | [Display Issues](./display-issues.md#character-encoding) |
| Wrong colors | [Display Issues](./display-issues.md#color-problems) |
| Slow scrolling | [Performance Issues](./performance-issues.md#scroll-performance) |
| High CPU | [Performance Issues](./performance-issues.md#cpu-usage) |
| Crash on start | [Platform Issues](./platform-issues.md) |

## Quick Fixes

### Reset to Defaults

```bash
# Backup current config
mv ~/.vellum-fe ~/.vellum-fe.backup

# Start fresh - VellumFE creates defaults
vellum-fe
```

### Force Reconnect

In VellumFE:
```
.reconnect
```

Or restart:
```bash
# Kill existing process
pkill vellum-fe

# Start fresh
vellum-fe
```

### Clear Cache

```bash
# Remove cached data
rm -rf ~/.vellum-fe/cache/

# Remove certificate (re-downloads)
rm ~/.vellum-fe/simu.pem
```

## Diagnostic Mode

Start with extra diagnostics:

```bash
# Maximum verbosity
TF_LOG=trace vellum-fe --debug

# Log to specific file
TF_LOG_FILE=/tmp/tf-debug.log vellum-fe
```

## See Also

- [Installation](../getting-started/installation.md) - Setup instructions
- [Configuration](../configuration/README.md) - Config reference
- [Network](../network/README.md) - Connection setup

