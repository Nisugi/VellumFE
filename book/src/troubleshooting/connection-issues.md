# Connection Issues

Solving network problems, authentication failures, and disconnections.

## Connection Modes

VellumFE supports two connection modes:

| Mode | Use When | Port | Requires |
|------|----------|------|----------|
| Lich | Using Lich scripts | 8000 (default) | Lich running |
| Direct | Standalone | 7910 | Account credentials |

## Quick Diagnosis

```bash
# Test Lich connection
nc -zv 127.0.0.1 8000

# Test direct connection
nc -zv eaccess.play.net 7910

# Check DNS
nslookup eaccess.play.net

# Full connectivity test
curl -v https://eaccess.play.net:7910 2>&1 | head -20
```

## Lich Mode Issues

### "Connection Refused"

**Symptom**:
```
Error: Connection refused to 127.0.0.1:8000
```

**Causes**:
1. Lich not running
2. Lich not listening on expected port
3. Firewall blocking localhost

**Solutions**:

1. **Start Lich first**:
   ```bash
   # Start Lich (method varies)
   ruby lich.rb
   # Or use your Lich launcher
   ```

2. **Check Lich is listening**:
   ```bash
   # Linux/macOS
   lsof -i :8000

   # Windows
   netstat -an | findstr 8000
   ```

3. **Verify port in Lich settings**:
   - Check Lich configuration for proxy port
   - Match in VellumFE:
     ```toml
     [connection]
     mode = "lich"
     port = 8000  # Match Lich's setting
     ```

### Can't Connect After Lich Starts

**Symptom**: Lich running but VellumFE can't connect

**Causes**:
1. Lich hasn't initialized proxy yet
2. Wrong interface binding

**Solutions**:

1. **Wait for Lich startup**:
   ```toml
   [connection]
   connect_delay = 3  # seconds to wait
   ```

2. **Check Lich's listen address**:
   ```toml
   # If Lich bound to specific interface
   [connection]
   host = "127.0.0.1"  # Must match Lich
   ```

### Disconnects When Lich Reloads

**Symptom**: Connection drops when Lich scripts reload

**Solution**:
```toml
[connection]
auto_reconnect = true
reconnect_delay = 2
```

## Direct Mode Issues

### Authentication Failed

**Symptom**:
```
Error: Authentication failed: invalid credentials
```

**Causes**:
1. Wrong account name
2. Wrong password
3. Special characters in password
4. Account locked/expired

**Solutions**:

1. **Verify credentials**:
   - Test via web login first
   - Check account status

2. **Handle special characters**:
   ```bash
   # Quote password with special chars
   vellum-fe --direct --account NAME --password 'P@ss!word'
   ```

3. **Use environment variables** (more secure):
   ```bash
   export TF_ACCOUNT="myaccount"
   export TF_PASSWORD="mypassword"
   vellum-fe --direct
   ```

4. **Check for account issues**:
   - Verify subscription is active
   - Check for account locks

### "Certificate Verification Failed"

**Symptom**:
```
Error: Certificate verification failed
```

**Causes**:
1. Corrupted cached certificate
2. Man-in-the-middle (security concern!)
3. Server certificate changed

**Solutions**:

1. **Remove and re-download certificate**:
   ```bash
   rm ~/.vellum-fe/simu.pem
   vellum-fe --direct ...  # Downloads fresh cert
   ```

2. **If error persists**:
   - Check if you're behind a corporate proxy
   - Verify you're on a trusted network
   - Certificate changes may indicate security issues

3. **Manual certificate verification**:
   ```bash
   openssl s_client -connect eaccess.play.net:7910 -servername eaccess.play.net
   ```

### "Connection Timed Out"

**Symptom**:
```
Error: Connection to eaccess.play.net:7910 timed out
```

**Causes**:
1. Firewall blocking
2. Network issues
3. Server maintenance

**Solutions**:

1. **Check basic connectivity**:
   ```bash
   ping eaccess.play.net
   traceroute eaccess.play.net
   ```

2. **Verify port access**:
   ```bash
   nc -zv eaccess.play.net 7910
   ```

3. **Check firewall**:
   ```bash
   # Linux - check if port 7910 is blocked
   sudo iptables -L -n | grep 7910

   # Windows - check firewall
   netsh advfirewall firewall show rule name=all | findstr 7910
   ```

4. **Increase timeout**:
   ```toml
   [connection]
   timeout = 60  # seconds
   ```

### "Character Not Found"

**Symptom**:
```
Error: Character 'Mychar' not found on account
```

**Solutions**:

1. **Check character name spelling** (case-sensitive)

2. **Verify character exists on correct game**:
   ```bash
   vellum-fe --direct --game prime ...  # GemStone IV Prime
   vellum-fe --direct --game plat ...   # GemStone IV Platinum
   vellum-fe --direct --game test ...   # Test server
   ```

3. **List available characters**:
   ```bash
   vellum-fe --direct --list-characters
   ```

## Connection Drops

### Random Disconnects

**Symptom**: Connection drops unexpectedly during play

**Causes**:
1. Network instability
2. Idle timeout
3. Server-side disconnection

**Solutions**:

1. **Enable auto-reconnect**:
   ```toml
   [connection]
   auto_reconnect = true
   reconnect_delay = 2
   reconnect_attempts = 5
   ```

2. **Send keepalives**:
   ```toml
   [connection]
   keepalive = true
   keepalive_interval = 30  # seconds
   ```

3. **Check for network issues**:
   ```bash
   # Monitor connection
   ping -i 5 gs4.play.net
   ```

### Disconnects During High Activity

**Symptom**: Drops during combat or busy areas

**Causes**:
1. Buffer overflow
2. Processing can't keep up
3. Network congestion

**Solutions**:

1. **Increase buffers**:
   ```toml
   [network]
   receive_buffer = 65536
   send_buffer = 32768
   ```

2. **Enable compression** (if supported):
   ```toml
   [network]
   compression = true
   ```

### "Connection Reset by Peer"

**Symptom**:
```
Error: Connection reset by peer
```

**Causes**:
1. Server closed connection
2. Network equipment reset
3. Rate limiting

**Solutions**:

1. **Check for rapid reconnects** (may be rate limited):
   ```toml
   [connection]
   reconnect_delay = 5  # Don't reconnect too fast
   ```

2. **Review recent actions**:
   - Excessive commands?
   - Script flooding?

## Firewall Issues

### Corporate/School Network

**Symptom**: Works at home, fails at work/school

**Solutions**:

1. **Check if port 7910 allowed**:
   - Direct mode uses port 7910 (non-standard)
   - May be blocked by corporate firewalls

2. **Use Lich as intermediary**:
   - If Lich works (different protocol), use Lich mode

3. **Request firewall exception**:
   - Port 7910 TCP outbound to eaccess.play.net

### VPN Issues

**Symptom**: Connection fails when VPN active

**Solutions**:

1. **Check VPN split tunneling**:
   - Gaming traffic might be routed through VPN
   - Configure VPN to exclude game servers

2. **DNS through VPN**:
   ```bash
   # Check DNS resolution
   nslookup eaccess.play.net
   ```

3. **Try without VPN** to confirm VPN is the issue

## Proxy Configuration

### HTTP Proxy

```toml
[network]
http_proxy = "http://proxy.example.com:8080"
```

### SOCKS Proxy

```toml
[network]
socks_proxy = "socks5://localhost:1080"
```

### No Proxy for Game Servers

```toml
[network]
no_proxy = "eaccess.play.net,gs4.play.net"
```

## SSL/TLS Issues

### "SSL Handshake Failed"

**Symptom**:
```
Error: SSL handshake failed
```

**Causes**:
1. TLS version mismatch
2. Cipher suite incompatibility
3. OpenSSL issues

**Solutions**:

1. **Check OpenSSL version**:
   ```bash
   openssl version
   # Should be 1.1.x or higher
   ```

2. **Force TLS version** (if needed):
   ```toml
   [network]
   tls_min_version = "1.2"
   ```

3. **Rebuild with updated OpenSSL**:
   ```bash
   # Ensure vcpkg OpenSSL is current
   vcpkg update
   vcpkg upgrade openssl
   ```

### "Certificate Chain" Errors

**Symptom**: Certificate chain validation errors

**Solution**:
```bash
# Reset certificate cache
rm ~/.vellum-fe/simu.pem
rm ~/.vellum-fe/cert_chain.pem

# Reconnect - will re-download
vellum-fe --direct ...
```

## Debugging Connections

### Enable Network Logging

```toml
[logging]
level = "debug"

[debug]
log_network = true
log_network_data = false  # true for packet dumps (verbose!)
```

### View Network Debug Info

```bash
# Start with network debugging
TF_DEBUG_NETWORK=1 vellum-fe

# Log to file for analysis
TF_LOG_FILE=/tmp/tf-network.log vellum-fe --debug
```

### Test Connection Step by Step

```bash
# 1. DNS resolution
nslookup eaccess.play.net

# 2. Basic connectivity
ping eaccess.play.net

# 3. Port availability
nc -zv eaccess.play.net 7910

# 4. TLS connection
openssl s_client -connect eaccess.play.net:7910

# 5. Full connection test
vellum-fe --direct --test-connection
```

## Quick Reference

### Connection Checklist

- [ ] Correct mode (lich vs direct)
- [ ] Lich running (if lich mode)
- [ ] Credentials correct (if direct mode)
- [ ] Port accessible (8000 for lich, 7910 for direct)
- [ ] Firewall allows connection
- [ ] Certificate valid
- [ ] Network stable

### Emergency Recovery

```bash
# Reset all connection state
rm ~/.vellum-fe/simu.pem      # Certificate cache
rm ~/.vellum-fe/session.dat   # Session cache

# Test with minimal config
vellum-fe --default-config --direct --account NAME --password PASS
```

## See Also

- [Network Overview](../network/README.md)
- [Lich Proxy](../network/lich-proxy.md)
- [Direct eAccess](../network/direct-eaccess.md)
- [TLS Certificates](../network/tls-certificates.md)

