# Direct eAccess Connection

Connect directly to GemStone IV and DragonRealms without requiring Lich as middleware.

## Overview

Direct eAccess mode allows VellumFE to authenticate directly with Simutronics' servers, bypassing the need for Lich. This provides:

- Standalone operation
- Lower latency
- Fewer dependencies
- Simpler setup

**Trade-off**: No Lich script support in this mode.

## Prerequisites

- OpenSSL library
- Game account and subscription
- Character already created

## Installation Requirements

### Windows

OpenSSL is installed via vcpkg during the build process:

1. Ensure `VCPKG_ROOT` environment variable is set
2. vcpkg handles OpenSSL installation automatically

If building from source:
```bash
vcpkg install openssl:x64-windows
```

### macOS

OpenSSL is typically available via Homebrew:

```bash
brew install openssl
```

### Linux

Install OpenSSL development package:

```bash
# Debian/Ubuntu
sudo apt install libssl-dev

# Fedora/RHEL
sudo dnf install openssl-devel

# Arch
sudo pacman -S openssl
```

## Connecting

### Command Line

```bash
vellum-fe --direct \
  --account YOUR_ACCOUNT \
  --password YOUR_PASSWORD \
  --game prime \
  --character CHARACTER_NAME
```

### Options

| Option | Description | Example |
|--------|-------------|---------|
| `--direct` | Enable direct eAccess mode | Required flag |
| `--account` | Simutronics account name | `myaccount` |
| `--password` | Account password | `mypassword` |
| `--game` | Game instance | `prime`, `test`, `dr` |
| `--character` | Character name | `Mycharacter` |

### Game Instance Values

| Value | Game |
|-------|------|
| `prime` | GemStone IV (Prime) |
| `test` | GemStone IV (Test) |
| `plat` | GemStone IV (Platinum) |
| `dr` | DragonRealms |
| `drt` | DragonRealms (Test) |

### Environment Variables

For security, credentials can be passed via environment:

```bash
export TF_ACCOUNT="myaccount"
export TF_PASSWORD="mypassword"
export TF_GAME="prime"
export TF_CHARACTER="Mycharacter"

vellum-fe --direct
```

## Authentication Flow

### How It Works

```
┌──────────────────────────────────────────────────────────────────┐
│                    Direct eAccess Flow                           │
│                                                                  │
│  ┌─────────┐    TLS     ┌─────────────┐                         │
│  │VellumFE │◄──────────▶│   eAccess   │  1. TLS Handshake       │
│  └─────────┘            │  7900/7910  │  2. Request hash key    │
│       │                 └─────────────┘  3. Send credentials    │
│       │                       │          4. Get character list  │
│       │                       │          5. Request launch key  │
│       │                       ▼                                  │
│       │                 ┌─────────────┐                         │
│       └────────────────▶│ Game Server │  6. Connect with key    │
│                         └─────────────┘                         │
└──────────────────────────────────────────────────────────────────┘
```

### Technical Details

1. **TLS Connection**: Connects to `eaccess.play.net:7910` using TLS
2. **Challenge-Response**:
   - Requests 32-byte hash key from server
   - Password is obfuscated: `((char - 32) ^ hashkey[i]) + 32`
3. **Session Creation**: Receives character list and subscription info
4. **Launch Ticket**: Requests and receives game server connection key
5. **Game Connection**: Connects to assigned game server with launch key

### Security Implementation

- **TLS Encryption**: All eAccess communication is encrypted
- **Certificate Pinning**: Server certificate is verified (stored at `~/.vellum-fe/simu.pem`)
- **No SNI**: Server Name Indication is disabled to match protocol requirements
- **Single-Write TLS**: Commands sent as single TLS records for compatibility

## Certificate Management

### First Connection

On first direct connection, VellumFE:
1. Downloads the eAccess server certificate
2. Stores it at `~/.vellum-fe/simu.pem`
3. Uses it for verification on subsequent connections

### Certificate Location

```
~/.vellum-fe/
└── simu.pem        # eAccess TLS certificate
```

### Refreshing Certificate

If authentication fails due to certificate issues:

```bash
# Delete stored certificate
rm ~/.vellum-fe/simu.pem

# Reconnect to download fresh certificate
vellum-fe --direct --account ... --password ... --game ... --character ...
```

## Configuration

### Config File (Partial)

You can specify some options in config, but credentials should not be stored:

```toml
# ~/.vellum-fe/config.toml
[connection]
mode = "direct"
game = "prime"

# Do NOT store credentials in config file!
```

### Recommended Approach

Use a wrapper script or shell alias:

```bash
# ~/.bashrc or ~/.zshrc
alias gs4='vellum-fe --direct --game prime --account $GS4_ACCOUNT --password $GS4_PASSWORD --character'

# Usage
gs4 Mycharacter
```

Or a script with secure password handling:

```bash
#!/bin/bash
# gs4-connect.sh

read -sp "Password: " PASSWORD
echo

vellum-fe --direct \
  --account "$1" \
  --password "$PASSWORD" \
  --game prime \
  --character "$2"
```

## Multi-Character

### Sequential Characters

Direct mode connects one character at a time. To switch:

1. Exit VellumFE
2. Reconnect with different `--character` parameter

### Parallel Characters

Run multiple VellumFE instances in separate terminals:

```bash
# Terminal 1
vellum-fe --direct --account myaccount --password mypass --game prime --character Warrior

# Terminal 2
vellum-fe --direct --account myaccount --password mypass --game prime --character Wizard
```

Each instance maintains its own connection.

## Performance

### Latency Comparison

| Mode | Typical Latency |
|------|-----------------|
| Direct | ~50-100ms (network only) |
| Lich | ~60-120ms (network + proxy) |

Direct mode eliminates the local proxy hop, providing marginally lower latency.

### Resource Usage

- **Memory**: ~20-30MB base (no Lich overhead)
- **CPU**: Minimal (only TLS processing)
- **Network**: Standard game traffic

## Troubleshooting

### Authentication Failed

```
Error: Authentication failed
```

**Solutions**:
1. Verify account credentials are correct
2. Test login via official client or Lich
3. Check account is active (not banned/suspended)
4. Delete and re-download certificate: `rm ~/.vellum-fe/simu.pem`

### Invalid Character

```
Error: Character not found
```

**Solutions**:
1. Check character name spelling (case-sensitive)
2. Verify character exists on specified game instance
3. Check account has access to that game

### Connection Timeout

```
Error: Connection to eaccess.play.net timed out
```

**Solutions**:
1. Check internet connectivity
2. Verify firewall allows port 7910 outbound
3. Try again (may be temporary server issue)

### TLS Handshake Failed

```
Error: TLS handshake failed
```

**Solutions**:
1. Update OpenSSL to latest version
2. Delete certificate and retry: `rm ~/.vellum-fe/simu.pem`
3. Check system time is correct (TLS requires accurate time)

### OpenSSL Not Found

```
Error: OpenSSL library not found
```

**Solutions**:
1. Install OpenSSL for your platform
2. Set library path if non-standard location
3. On Windows, verify vcpkg installation

## Security Considerations

### Credential Handling

- **Never store passwords in config files**
- Use environment variables or interactive prompts
- Clear shell history if credentials were typed

### Network Security

- All eAccess traffic is TLS encrypted
- Game server traffic uses standard (unencrypted) protocol
- Certificate pinning prevents MITM attacks on authentication

### Logging

Debug logs may contain sensitive information:
- Don't share logs publicly without redaction
- Logs are stored in `~/.vellum-fe/vellum-fe.log`

## Comparison with Lich Mode

| Aspect | Direct eAccess | Lich Proxy |
|--------|----------------|------------|
| Scripts | ✗ None | ✓ Full Lich scripts |
| Setup | Simple | Medium |
| Dependencies | OpenSSL | Lich + Ruby |
| Latency | Lower | Slightly higher |
| Memory | Lower | Higher |
| Multi-char | Separate instances | Lich manages |
| Community | Limited | Large ecosystem |

## When to Use Direct Mode

**Good for**:
- Players who don't use scripts
- Minimal installations
- Performance-sensitive setups
- Learning/testing the game

**Not recommended for**:
- Heavy script users
- Automation needs
- Community script ecosystem access

## See Also

- [Lich Proxy](./lich-proxy.md) - Alternative with script support
- [TLS Certificates](./tls-certificates.md) - Certificate details
- [Troubleshooting](./troubleshooting.md) - More connection issues

