# Lich Proxy Connection

Connect VellumFE through Lich for full script support and the Lich ecosystem.

## Overview

Lich is a middleware application that sits between your client and the game server. It provides:

- Script execution (Ruby-based automation)
- Plugin support
- Community scripts repository
- Character management

VellumFE connects to Lich's local proxy port, receiving game data and sending commands through Lich.

## Prerequisites

- Lich installed and configured
- Ruby (required by Lich)
- Game account and subscription

## Installing Lich

### Download Lich

Visit the Lich Project website or community resources to download Lich.

### Install Ruby

Lich requires Ruby. Install the appropriate version for your platform:

**Windows**:
- Download RubyInstaller from https://rubyinstaller.org/
- Install with DevKit option

**macOS**:
```bash
brew install ruby
```

**Linux**:
```bash
sudo apt install ruby ruby-dev
```

### Configure Lich

1. Run Lich setup wizard
2. Enter your Simutronics credentials
3. Select your character
4. Configure proxy port (default: 8000)

## Connecting VellumFE

### Basic Connection

Once Lich is running and logged in:

```bash
vellum-fe --host 127.0.0.1 --port 8000
```

### Configuration File

Add to `~/.vellum-fe/config.toml`:

```toml
[connection]
mode = "lich"
host = "127.0.0.1"
port = 8000
auto_reconnect = true
reconnect_delay = 5
```

### Command Line Options

```bash
vellum-fe --host HOST --port PORT [OPTIONS]

Options:
  --host HOST      Lich proxy host (default: 127.0.0.1)
  --port PORT      Lich proxy port (default: 8000)
  --no-reconnect   Disable auto-reconnect
```

## Connection Workflow

### Step-by-Step

1. **Start Lich**
   ```bash
   ruby lich.rb
   ```

2. **Log into character via Lich**
   - Use Lich's login interface
   - Select character
   - Wait for game connection

3. **Verify Lich is listening**
   - Lich should show proxy port status
   - Default: listening on port 8000

4. **Connect VellumFE**
   ```bash
   vellum-fe --host 127.0.0.1 --port 8000
   ```

5. **Verify connection**
   - VellumFE should display game output
   - Commands typed in VellumFE should work in game

### Diagram

```
┌──────────────────────────────────────────────────────────────┐
│                       Your Computer                          │
│                                                              │
│  ┌───────────┐         ┌──────────┐         ┌──────────┐    │
│  │ VellumFE  │◄───────▶│   Lich   │◄───────▶│ Scripts  │    │
│  └───────────┘         └──────────┘         └──────────┘    │
│       │                     │                               │
│       │                     │ Port 8000                     │
│       └─────────────────────┘                               │
│                             │                               │
└─────────────────────────────┼───────────────────────────────┘
                              │
                              ▼ Internet
                    ┌──────────────────┐
                    │   Game Server    │
                    └──────────────────┘
```

## Multi-Character Setup

### Different Ports per Character

If running multiple characters:

**Lich configuration**:
- Character 1: Port 8000
- Character 2: Port 8001
- Character 3: Port 8002

**VellumFE instances**:
```bash
# Terminal 1
vellum-fe --port 8000

# Terminal 2
vellum-fe --port 8001

# Terminal 3
vellum-fe --port 8002
```

### Profile-Based Configuration

Create character profiles in `~/.vellum-fe/profiles/`:

```toml
# ~/.vellum-fe/profiles/warrior.toml
[connection]
port = 8000

# ~/.vellum-fe/profiles/wizard.toml
[connection]
port = 8001
```

Launch with profile:
```bash
vellum-fe --profile warrior
vellum-fe --profile wizard
```

## Lich Script Integration

### Running Scripts

Lich scripts are controlled through Lich, not VellumFE. Common commands:

```
;script_name          # Run a script
;kill script_name     # Stop a script
;pause script_name    # Pause a script
;unpause script_name  # Resume a script
;list                 # List running scripts
```

These commands are sent to Lich, which executes them.

### Script Output

Script output appears in VellumFE's main text window alongside game output. You can filter script messages using stream filtering if Lich tags them appropriately.

### Script Commands vs Game Commands

| Prefix | Destination | Example |
|--------|-------------|---------|
| `;` | Lich (scripts) | `;go2 bank` |
| `.` | VellumFE (client) | `.reload config` |
| (none) | Game server | `north` |

## Lich Settings

### Proxy Configuration

In Lich's configuration:

```ruby
# Listen on all interfaces (for remote access)
proxy_host = "0.0.0.0"
proxy_port = 8000

# Listen on localhost only (more secure)
proxy_host = "127.0.0.1"
proxy_port = 8000
```

### XML Mode

Ensure Lich is configured to pass through game XML:

```ruby
xml_passthrough = true
```

VellumFE requires XML tags to properly parse game state.

## Performance Considerations

### Latency

Lich adds minimal latency (typically <10ms) due to local proxy processing.

### Memory

Running Lich + Ruby + scripts uses additional memory. Typical usage:
- Lich base: ~50-100MB
- Per character: +20-50MB
- Scripts: Varies by script

### CPU

Script execution uses CPU. Complex scripts may impact performance during heavy automation.

## Troubleshooting

### Connection Refused

```
Error: Connection refused to 127.0.0.1:8000
```

**Solutions**:
1. Verify Lich is running
2. Check Lich has logged into a character
3. Confirm proxy port matches (check Lich status)
4. Try a different port if 8000 is in use

### Lich Not Receiving Commands

**Solutions**:
1. Check VellumFE is connected (look for game output)
2. Verify commands aren't being intercepted
3. Check for Lich script conflicts

### XML Not Parsing

If widgets aren't updating:

**Solutions**:
1. Enable XML passthrough in Lich
2. Check Lich version supports XML
3. Verify game has XML mode enabled

### Disconnects

If connection drops frequently:

**Solutions**:
1. Check network stability
2. Enable auto-reconnect:
   ```toml
   [connection]
   auto_reconnect = true
   reconnect_delay = 5
   ```
3. Check Lich logs for errors

### Port Already in Use

```
Error: Port 8000 already in use
```

**Solutions**:
1. Close other applications using the port
2. Change Lich's proxy port
3. Find the process using the port:
   ```bash
   # Linux/macOS
   lsof -i :8000

   # Windows
   netstat -ano | findstr :8000
   ```

## Advanced Configuration

### Remote Lich Access

To connect from a different machine (advanced):

1. Configure Lich to listen on network:
   ```ruby
   proxy_host = "0.0.0.0"
   ```

2. Connect from VellumFE:
   ```bash
   vellum-fe --host 192.168.1.100 --port 8000
   ```

3. **Security warning**: Only do this on trusted networks!

### SSH Tunneling

For secure remote access:

```bash
# On remote machine
ssh -L 8000:localhost:8000 user@lich-server

# Then connect VellumFE locally
vellum-fe --host 127.0.0.1 --port 8000
```

## See Also

- [Direct eAccess](./direct-eaccess.md) - Alternative without Lich
- [Troubleshooting](./troubleshooting.md) - More connection issues
- [Configuration](../configuration/config-toml.md) - Full config reference

