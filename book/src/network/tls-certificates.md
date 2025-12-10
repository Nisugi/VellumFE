# TLS Certificates

Understanding and managing TLS certificates for secure connections.

## Overview

VellumFE uses TLS (Transport Layer Security) for encrypted communication with Simutronics' eAccess authentication servers. This page covers certificate management for direct eAccess mode.

## Certificate Architecture

### Connection Security

```
┌─────────────────────────────────────────────────────────────┐
│                  TLS Connection Flow                        │
│                                                             │
│  VellumFE                    eAccess Server                 │
│     │                             │                         │
│     │──── ClientHello ───────────▶│                         │
│     │                             │                         │
│     │◀─── ServerHello + Cert ─────│                         │
│     │                             │                         │
│     │  Verify cert against        │                         │
│     │  stored simu.pem            │                         │
│     │                             │                         │
│     │──── Key Exchange ──────────▶│                         │
│     │                             │                         │
│     │◀═══ Encrypted Channel ═════▶│                         │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Why Certificate Pinning?

The eAccess server uses a self-signed certificate. Certificate pinning:

1. **First connection**: Downloads and stores the certificate
2. **Subsequent connections**: Verifies server presents the same certificate
3. **Protection**: Prevents man-in-the-middle attacks

## Certificate Storage

### Location

```
~/.vellum-fe/
└── simu.pem        # eAccess server certificate
```

### File Format

The certificate is stored in PEM format:

```
-----BEGIN CERTIFICATE-----
MIIDxTCCAq2gAwIBAgIJAK... (base64 encoded)
-----END CERTIFICATE-----
```

### Permissions

The certificate file should have restricted permissions:

```bash
# Linux/macOS
chmod 600 ~/.vellum-fe/simu.pem

# Windows
# File inherits user permissions from .vellum-fe folder
```

## Certificate Lifecycle

### First Connection

On first direct eAccess connection:

1. VellumFE connects to `eaccess.play.net:7910`
2. Server sends its certificate during TLS handshake
3. VellumFE saves certificate to `~/.vellum-fe/simu.pem`
4. Connection continues with authentication

### Subsequent Connections

On later connections:

1. VellumFE loads stored certificate
2. During TLS handshake, compares server's certificate
3. If match: connection proceeds
4. If mismatch: connection fails (security protection)

### Certificate Renewal

If Simutronics updates their certificate:

1. Connection will fail (certificate mismatch)
2. Delete the old certificate: `rm ~/.vellum-fe/simu.pem`
3. Reconnect to download new certificate
4. Future connections use new certificate

## Managing Certificates

### Viewing Certificate

```bash
# View certificate details
openssl x509 -in ~/.vellum-fe/simu.pem -text -noout

# View expiration date
openssl x509 -in ~/.vellum-fe/simu.pem -enddate -noout

# View fingerprint
openssl x509 -in ~/.vellum-fe/simu.pem -fingerprint -noout
```

### Example Output

```
Certificate:
    Data:
        Version: 3 (0x2)
        Serial Number: ...
        Signature Algorithm: sha256WithRSAEncryption
        Issuer: C=US, ST=..., O=Simutronics...
        Validity
            Not Before: Jan  1 00:00:00 2020 GMT
            Not After : Dec 31 23:59:59 2030 GMT
        Subject: CN=eaccess.play.net...
```

### Deleting Certificate

To force certificate refresh:

```bash
# Linux/macOS
rm ~/.vellum-fe/simu.pem

# Windows (PowerShell)
Remove-Item ~\.vellum-fe\simu.pem

# Windows (Command Prompt)
del %USERPROFILE%\.vellum-fe\simu.pem
```

### Backing Up Certificate

If you want to preserve your certificate:

```bash
cp ~/.vellum-fe/simu.pem ~/.vellum-fe/simu.pem.backup
```

## Troubleshooting

### Certificate Verification Failed

```
Error: Certificate verification failed
```

**Causes**:
- Certificate changed on server
- Certificate file corrupted
- System time incorrect

**Solutions**:
1. Delete and re-download:
   ```bash
   rm ~/.vellum-fe/simu.pem
   ```
2. Check system time is accurate
3. Verify network isn't intercepting traffic

### Certificate Not Found

```
Error: Could not load certificate from ~/.vellum-fe/simu.pem
```

**Causes**:
- First connection hasn't occurred
- Certificate was deleted
- Permission issues

**Solutions**:
1. Run direct connection to auto-download
2. Check folder permissions
3. Verify path is correct

### TLS Handshake Error

```
Error: TLS handshake failed
```

**Causes**:
- OpenSSL version incompatibility
- Network proxy interference
- Server configuration changed

**Solutions**:
1. Update OpenSSL
2. Disable network proxies
3. Delete certificate and retry

### Self-Signed Certificate Warning

The eAccess certificate is self-signed, which is expected. VellumFE handles this through certificate pinning rather than chain validation.

## Advanced Topics

### Manual Certificate Download

If auto-download fails, manually retrieve:

```bash
# Connect and save certificate
openssl s_client -connect eaccess.play.net:7910 \
  -servername "" \
  </dev/null 2>/dev/null | \
  openssl x509 -outform PEM > ~/.vellum-fe/simu.pem
```

Note: The empty `-servername ""` disables SNI, which is required.

### Certificate Validation

To manually verify a certificate:

```bash
# Compare fingerprints
openssl x509 -in ~/.vellum-fe/simu.pem -fingerprint -sha256 -noout
```

Compare the fingerprint with known-good values from the community.

### Multiple Certificates

If connecting to different game servers (test vs production):

```
~/.vellum-fe/
├── simu.pem           # Production eAccess
├── simu-test.pem      # Test server (if different)
```

Configure via:
```toml
[connection]
certificate = "simu.pem"  # or "simu-test.pem"
```

## Security Best Practices

### Do

- ✓ Keep certificate file permissions restrictive
- ✓ Verify certificate fingerprint periodically
- ✓ Delete and refresh if authentication fails unexpectedly
- ✓ Keep OpenSSL updated

### Don't

- ✗ Share your certificate file publicly
- ✗ Ignore certificate verification failures
- ✗ Use the same certificate across different machines (download fresh)
- ✗ Disable certificate verification

## Technical Details

### TLS Configuration

VellumFE's TLS connection uses:

| Setting | Value | Reason |
|---------|-------|--------|
| Protocol | TLS 1.2+ | Security |
| SNI | Disabled | Server requirement |
| Session caching | Disabled | Protocol compatibility |
| Cipher suites | System default | OpenSSL manages |

### Single-Write Requirement

A critical implementation detail: commands must be sent as single TLS Application Data records. VellumFE ensures this by building complete messages in memory before writing to the TLS stream.

```rust
// Correct: Single write
let message = format!("{}\n", line);
stream.write_all(message.as_bytes())?;

// Incorrect: Multiple writes (would fail)
stream.write_all(line.as_bytes())?;
stream.write_all(b"\n")?;
```

## See Also

- [Direct eAccess](./direct-eaccess.md) - Direct connection setup
- [Troubleshooting](./troubleshooting.md) - Connection problems
- [Network Overview](./README.md) - Connection modes

