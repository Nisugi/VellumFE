# Network Overview

VellumFE supports two connection modes for connecting to GemStone IV and DragonRealms servers.

## Connection Modes

| Mode | Description | Use Case |
|------|-------------|----------|
| **Lich Proxy** | Connect through Lich middleware | Standard setup, script support |
| **Direct eAccess** | Authenticate directly with Simutronics | Standalone, no Lich required |

## Quick Comparison

| Feature | Lich Proxy | Direct eAccess |
|---------|------------|----------------|
| Lich scripts | ✓ Yes | ✗ No |
| Setup complexity | Medium | Simple |
| Dependencies | Lich, Ruby | OpenSSL |
| Authentication | Handled by Lich | Built-in |
| Latency | Slightly higher | Minimal |

## Connection Flow

### Lich Proxy Mode

```
┌─────────┐     ┌──────┐     ┌────────────┐     ┌──────────┐
│VellumFE │────▶│ Lich │────▶│ Game Server │────▶│ Response │
└─────────┘     └──────┘     └────────────┘     └──────────┘
     ▲              │                                │
     └──────────────┴────────────────────────────────┘
```

VellumFE connects to Lich's local proxy port (typically 8000). Lich handles authentication and provides script execution capabilities.

### Direct eAccess Mode

```
┌─────────┐     ┌─────────┐     ┌────────────┐
│VellumFE │────▶│ eAccess │────▶│ Game Server │
└─────────┘     └─────────┘     └────────────┘
     │               │                │
     └───────────────┴────────────────┘
         Direct TLS connection
```

VellumFE authenticates directly with Simutronics' eAccess servers, then connects to the game server.

## Which Mode Should I Use?

### Use Lich Proxy If:

- You use Lich scripts (recommended for most players)
- You want the Lich ecosystem (scripts, plugins)
- You're already familiar with Lich
- You need advanced automation

### Use Direct eAccess If:

- You want a standalone client
- You don't use Lich scripts
- You want minimal latency
- You prefer fewer dependencies
- Lich isn't available for your platform

## Network Requirements

### Ports

| Service | Port | Protocol |
|---------|------|----------|
| Lich proxy | 8000 (configurable) | TCP |
| eAccess | 7900/7910 | TLS |
| Game servers | Various | TCP |

### Firewall Rules

If behind a firewall, ensure outbound connections to:
- `eaccess.play.net` (port 7900/7910)
- Game server IPs (assigned dynamically)

## Security

### Lich Mode

- Lich handles credential storage
- Connection to Lich is local (127.0.0.1)
- Game traffic may or may not be encrypted depending on Lich version

### Direct Mode

- TLS encryption to eAccess
- Certificate pinning for authentication servers
- Credentials passed at runtime (not stored)

## Configuration

### Lich Mode

```toml
# config.toml
[connection]
mode = "lich"
host = "127.0.0.1"
port = 8000
```

### Direct Mode

```toml
# config.toml
[connection]
mode = "direct"
game = "gs4"          # or "dr" for DragonRealms
```

Credentials are provided via command line or environment variables (never stored in config).

## Guides

This section covers:

- [Lich Proxy](./lich-proxy.md) - Setting up Lich connection
- [Direct eAccess](./direct-eaccess.md) - Direct authentication
- [TLS Certificates](./tls-certificates.md) - Certificate management
- [Troubleshooting](./troubleshooting.md) - Connection problems

## Quick Start

### Lich Mode (Recommended)

1. Install and configure Lich
2. Start Lich and log into your character
3. Launch VellumFE: `vellum-fe --host 127.0.0.1 --port 8000`

### Direct Mode

1. Launch VellumFE with credentials:
   ```bash
   vellum-fe --direct \
     --account YOUR_ACCOUNT \
     --password YOUR_PASSWORD \
     --game prime \
     --character CHARACTER_NAME
   ```

## See Also

- [Installation](../getting-started/installation.md)
- [First Launch](../getting-started/first-launch.md)
- [Configuration](../configuration/config-toml.md)

