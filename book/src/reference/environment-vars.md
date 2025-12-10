# Environment Variables

Reference of all environment variables used by VellumFE.

## Connection Variables

### Direct eAccess Mode

| Variable | Description | Required |
|----------|-------------|----------|
| `TF_ACCOUNT` | Simutronics account name | Yes (if `--account` not provided) |
| `TF_PASSWORD` | Account password | Yes (if `--password` not provided) |
| `TF_GAME` | Game instance | Yes (if `--game` not provided) |
| `TF_CHARACTER` | Character name | Yes (if `--character` not provided) |

```bash
# Set credentials via environment
export TF_ACCOUNT="myaccount"
export TF_PASSWORD="mypassword"
export TF_GAME="prime"
export TF_CHARACTER="Warrior"

# Launch without command-line credentials
vellum-fe --direct
```

### Lich Mode

| Variable | Description | Default |
|----------|-------------|---------|
| `TF_HOST` | Lich proxy host | `127.0.0.1` |
| `TF_PORT` | Lich proxy port | `8000` |

```bash
export TF_HOST="192.168.1.100"
export TF_PORT="8001"
vellum-fe  # Uses environment values
```

## Configuration Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `TF_CONFIG_DIR` | Configuration directory | `~/.vellum-fe` |
| `TF_CONFIG` | Main config file path | `$TF_CONFIG_DIR/config.toml` |
| `TF_LAYOUT` | Layout config path | `$TF_CONFIG_DIR/layout.toml` |
| `TF_COLORS` | Colors config path | `$TF_CONFIG_DIR/colors.toml` |
| `TF_KEYBINDS` | Keybinds config path | `$TF_CONFIG_DIR/keybinds.toml` |

```bash
# Custom config directory
export TF_CONFIG_DIR="/home/user/games/vellum-fe"

# Individual file overrides
export TF_LAYOUT="/home/user/layouts/hunting.toml"
```

## Logging Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `TF_LOG_LEVEL` | Log verbosity | `info` |
| `TF_LOG_FILE` | Log file path | `$TF_CONFIG_DIR/vellum-fe.log` |
| `RUST_LOG` | Rust logging (alternative) | - |

```bash
# Enable debug logging
export TF_LOG_LEVEL="debug"

# Custom log file
export TF_LOG_FILE="/tmp/vellum-fe.log"

# Or use Rust's standard logging
export RUST_LOG="vellum_fe=debug"
```

### Log Levels

| Level | Description |
|-------|-------------|
| `error` | Errors only |
| `warn` | Warnings and above |
| `info` | General information |
| `debug` | Debug information |
| `trace` | Verbose tracing |

## Display Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `TF_NO_COLOR` | Disable colors | `false` |
| `TF_FORCE_COLOR` | Force colors | `false` |
| `TERM` | Terminal type | System default |
| `COLORTERM` | Color support level | System default |

```bash
# Disable colors (for logging/piping)
export TF_NO_COLOR=1

# Force colors (for terminals that don't advertise support)
export TF_FORCE_COLOR=1
```

## Build Variables

Used during compilation:

| Variable | Description |
|----------|-------------|
| `VCPKG_ROOT` | vcpkg installation path (Windows) |
| `OPENSSL_DIR` | OpenSSL installation path |
| `OPENSSL_LIB_DIR` | OpenSSL library path |
| `OPENSSL_INCLUDE_DIR` | OpenSSL headers path |

```bash
# Windows (vcpkg)
set VCPKG_ROOT=C:\tools\vcpkg

# macOS (Homebrew)
export OPENSSL_DIR=$(brew --prefix openssl)

# Linux (custom OpenSSL)
export OPENSSL_DIR=/opt/openssl
export OPENSSL_LIB_DIR=$OPENSSL_DIR/lib
export OPENSSL_INCLUDE_DIR=$OPENSSL_DIR/include
```

## TTS Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `TF_TTS_ENGINE` | TTS engine override | System default |
| `TF_TTS_VOICE` | Voice override | System default |

```bash
# Force specific TTS engine
export TF_TTS_ENGINE="espeak"
export TF_TTS_VOICE="en-us"
```

## Precedence

Environment variables are applied in this order:

1. Default values (lowest)
2. Configuration files
3. **Environment variables**
4. Command-line arguments (highest)

Example:

```bash
# Config file says port = 8000
# Environment says TF_PORT=8001
# Command line says --port 8002

# Result: port 8002 is used
```

## Security Notes

### Credentials in Environment

Environment variables are visible to:
- The current process
- Child processes
- Process inspection tools

**Recommendations**:
- Don't export credentials in shell profile files
- Use temporary environment for session
- Clear history after entering passwords

```bash
# Safer approach - prompt for password
read -sp "Password: " TF_PASSWORD
export TF_PASSWORD
```

### Logging Redaction

Logs may contain environment values. The log system attempts to redact:
- `TF_PASSWORD`
- `TF_ACCOUNT` (partially)

But review logs before sharing publicly.

## Shell Integration

### Bash/Zsh

```bash
# ~/.bashrc or ~/.zshrc

# VellumFE config
export TF_CONFIG_DIR="$HOME/.vellum-fe"
export TF_LOG_LEVEL="info"

# Alias with common options
alias gs4='vellum-fe --host 127.0.0.1 --port 8000'
```

### Fish

```fish
# ~/.config/fish/config.fish

set -x TF_CONFIG_DIR "$HOME/.vellum-fe"
set -x TF_LOG_LEVEL "info"

alias gs4 'vellum-fe --host 127.0.0.1 --port 8000'
```

### PowerShell

```powershell
# $PROFILE

$env:TF_CONFIG_DIR = "$env:USERPROFILE\.vellum-fe"
$env:TF_LOG_LEVEL = "info"

function gs4 { vellum-fe --host 127.0.0.1 --port 8000 $args }
```

### Windows Command Prompt

```batch
:: Set for current session
set TF_CONFIG_DIR=%USERPROFILE%\.vellum-fe

:: Set permanently (requires admin)
setx TF_CONFIG_DIR "%USERPROFILE%\.vellum-fe"
```

## Debugging

Check current environment:

```bash
# Show all TF_ variables
env | grep ^TF_

# Show specific variable
echo $TF_CONFIG_DIR
```

## See Also

- [CLI Options](./cli-options.md)
- [Configuration](../configuration/README.md)
- [Building](../development/building.md)

