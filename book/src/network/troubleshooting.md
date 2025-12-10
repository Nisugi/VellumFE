# Network Troubleshooting

Solutions for common connection issues with VellumFE.

## Quick Diagnostics

### Check Connection Status

```bash
# Verify VellumFE is running
ps aux | grep vellum-fe

# Check if port is in use (Lich mode)
netstat -an | grep 8000

# Test internet connectivity
ping eaccess.play.net
```

### View Logs

```bash
# Tail the log file
tail -f ~/.vellum-fe/vellum-fe.log

# Search for errors
grep -i error ~/.vellum-fe/vellum-fe.log
```

## Common Issues

### Connection Refused

**Symptom**:
```
Error: Connection refused to 127.0.0.1:8000
```

**Cause**: Lich proxy isn't running or listening on the expected port.

**Solutions**:

1. **Verify Lich is running**:
   ```bash
   # Check for Lich process
   ps aux | grep -i lich
   ```

2. **Check Lich has connected to game**:
   - Lich must be logged into a character
   - The proxy only starts after game connection

3. **Verify port number**:
   - Check Lich's configured proxy port
   - Default is 8000, but may be different

4. **Try a different port**:
   ```bash
   # If 8000 is blocked, configure Lich for 8001
   vellum-fe --port 8001
   ```

### Connection Timeout

**Symptom**:
```
Error: Connection timed out
```

**Cause**: Network issue between VellumFE and the target.

**Solutions**:

1. **Lich mode** - Check Lich is responding:
   ```bash
   telnet 127.0.0.1 8000
   ```

2. **Direct mode** - Check internet connectivity:
   ```bash
   # Test eAccess server
   nc -zv eaccess.play.net 7910

   # Or with OpenSSL
   openssl s_client -connect eaccess.play.net:7910
   ```

3. **Firewall check**:
   - Ensure port 7910 is allowed outbound
   - Check corporate/school firewall policies

4. **Try again later**:
   - Simutronics servers may be temporarily unavailable
   - Check official forums for server status

### Authentication Failed

**Symptom**:
```
Error: Authentication failed
```

**Cause**: Invalid credentials or account issue.

**Solutions**:

1. **Verify credentials**:
   - Double-check account name (case-insensitive)
   - Verify password (case-sensitive)
   - Test login via official client first

2. **Account status**:
   - Ensure subscription is active
   - Check account isn't banned/suspended
   - Verify no billing issues

3. **Character name**:
   - Confirm character exists
   - Check spelling (often case-sensitive)
   - Verify character is on correct game instance

4. **Certificate issue** (direct mode):
   ```bash
   # Reset certificate
   rm ~/.vellum-fe/simu.pem
   # Retry connection
   ```

### TLS Handshake Failed

**Symptom**:
```
Error: TLS handshake failed
```

**Cause**: SSL/TLS configuration issue.

**Solutions**:

1. **Update OpenSSL**:
   ```bash
   # Check version
   openssl version

   # Update (varies by OS)
   # macOS: brew upgrade openssl
   # Ubuntu: sudo apt update && sudo apt upgrade openssl
   ```

2. **Reset certificate**:
   ```bash
   rm ~/.vellum-fe/simu.pem
   ```

3. **Check system time**:
   - TLS requires accurate system time
   - Sync with NTP server

4. **Network proxy**:
   - Disable SSL inspection proxies
   - Corporate proxies may intercept TLS

### Character Not Found

**Symptom**:
```
Error: Character 'Mychar' not found on account
```

**Cause**: Character doesn't exist or wrong game instance.

**Solutions**:

1. **Check character name spelling**:
   - Some systems are case-sensitive
   - No spaces or special characters

2. **Verify game instance**:
   ```bash
   # GemStone IV Prime
   --game prime

   # GemStone IV Platinum
   --game plat

   # GemStone IV Test
   --game test

   # DragonRealms
   --game dr
   ```

3. **List characters**:
   - Login via official client to see character list
   - Verify account has characters on that game

### Disconnected Unexpectedly

**Symptom**: Connection drops during gameplay.

**Cause**: Network instability, server kick, or timeout.

**Solutions**:

1. **Enable auto-reconnect**:
   ```toml
   [connection]
   auto_reconnect = true
   reconnect_delay = 5
   ```

2. **Check network stability**:
   ```bash
   # Monitor packet loss
   ping -c 100 eaccess.play.net
   ```

3. **Idle timeout**:
   - Game may disconnect idle connections
   - Configure keepalive if available

4. **Server-side kick**:
   - Check for policy violations
   - Too many connections from same IP
   - Anti-automation detection

### Port Already in Use

**Symptom**:
```
Error: Address already in use: 0.0.0.0:8000
```

**Cause**: Another application is using the port.

**Solutions**:

1. **Find the process**:
   ```bash
   # Linux/macOS
   lsof -i :8000

   # Windows
   netstat -ano | findstr :8000
   ```

2. **Kill the process**:
   ```bash
   # Linux/macOS
   kill -9 <PID>

   # Windows
   taskkill /F /PID <PID>
   ```

3. **Use different port**:
   - Configure Lich for different port
   - Connect VellumFE to new port

### No Data Received

**Symptom**: Connected but no game output appears.

**Cause**: Stream configuration or parsing issue.

**Solutions**:

1. **Check stream configuration**:
   ```toml
   # Ensure main window has appropriate streams
   [[widgets]]
   type = "text"
   streams = ["main", "room", "combat"]
   ```

2. **Verify XML passthrough** (Lich mode):
   - Lich must pass through game XML
   - Check Lich configuration

3. **Test with simple layout**:
   ```toml
   # Minimal test layout
   [[widgets]]
   type = "text"
   streams = []  # Empty = all streams
   x = 0
   y = 0
   width = 100
   height = 100
   ```

4. **Check logs for parsing errors**:
   ```bash
   grep -i parse ~/.vellum-fe/vellum-fe.log
   ```

### SSL Certificate Error

**Symptom**:
```
Error: SSL certificate problem: self signed certificate
```

**Cause**: Certificate verification issue (expected for eAccess).

**Solutions**:

1. This is normal for first connection - certificate should be pinned automatically

2. If persistent, reset certificate:
   ```bash
   rm ~/.vellum-fe/simu.pem
   ```

3. Check no MITM proxy is intercepting traffic

## Platform-Specific Issues

### Windows

**OpenSSL not found**:
```bash
# Ensure vcpkg installed OpenSSL
vcpkg list | findstr openssl

# Set VCPKG_ROOT
set VCPKG_ROOT=C:\path\to\vcpkg
```

**Permission denied on config**:
- Run as Administrator once to create directories
- Or manually create `%USERPROFILE%\.vellum-fe\`

### macOS

**OpenSSL version mismatch**:
```bash
# Use Homebrew OpenSSL
export OPENSSL_DIR=$(brew --prefix openssl)

# Or install specific version
brew install openssl@3
```

**Keychain access**:
- macOS may prompt for keychain access
- Allow VellumFE to access certificates

### Linux

**Missing libraries**:
```bash
# Debian/Ubuntu
sudo apt install libssl-dev ca-certificates

# Fedora
sudo dnf install openssl-devel ca-certificates

# Arch
sudo pacman -S openssl
```

**SELinux blocking**:
```bash
# Check for denials
ausearch -m AVC -ts recent

# Temporarily permissive (testing only)
sudo setenforce 0
```

## Advanced Diagnostics

### Network Capture

```bash
# Capture Lich traffic
tcpdump -i lo port 8000 -w lich_traffic.pcap

# Capture eAccess traffic
tcpdump -i eth0 host eaccess.play.net -w eaccess_traffic.pcap
```

### Debug Logging

Enable verbose logging:

```toml
# config.toml
[logging]
level = "debug"
file = "~/.vellum-fe/debug.log"
```

### Test Mode

Some issues can be isolated with test connections:

```bash
# Test Lich connection
echo "look" | nc localhost 8000

# Test eAccess with OpenSSL
openssl s_client -connect eaccess.play.net:7910 -quiet
```

## Getting Help

If issues persist:

1. **Check logs**: `~/.vellum-fe/vellum-fe.log`
2. **Reproduce minimally**: Simple config, default layout
3. **Search issues**: Check GitHub issues for similar problems
4. **Report bug**: Include:
   - VellumFE version
   - OS and version
   - Connection mode
   - Relevant log excerpts (redact credentials!)
   - Steps to reproduce

## See Also

- [Lich Proxy](./lich-proxy.md) - Lich connection details
- [Direct eAccess](./direct-eaccess.md) - Direct connection details
- [TLS Certificates](./tls-certificates.md) - Certificate management

