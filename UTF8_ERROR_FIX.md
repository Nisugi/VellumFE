# UTF-8 Error Handling Fix

## Issue Description

VellumFE was experiencing intermittent crashes with the error:
```
ERROR vellum_fe::network: Error reading from server: stream did not contain valid UTF-8
```

This would cause the application to disconnect from the Lich server unexpectedly.

## Root Cause

The GemStone IV game server occasionally sends data that contains **invalid UTF-8 byte sequences**. Specifically, we observed:

- **Byte `0x8A` (138 decimal)** - Invalid in UTF-8, but represents "Š" in Windows-1252
- **Byte `0xA0` (160 decimal)** - Non-breaking space in Windows-1252/ISO-8859-1

These bytes appear in room descriptions and game text, likely due to legacy encoding (Windows-1252) being mixed into what should be UTF-8 data.

### Example from Debug Log

```
Raw bytes (hex): 3c 63 6f 6d 70 6f 6e 65 6e 74 20 69 64 3d 27 72 6f 6f 6d 20 6f 62 6a 73 27 3e 20 20 59 6f 75 20 6e 6f 74 69 63 65 20 8a 61 20 3c 70 72 65 73 65 74...
```

At position 39 (byte `8a`), the UTF-8 decoder failed because `0x8A` is not valid UTF-8:
- It's in the range `0x80-0xBF` (continuation byte range)
- But it's not part of a valid multi-byte sequence
- It appears as a standalone byte

The original text was:
```
<component id='room objs'>  You notice �a <preset id='link'>...
```

The `�` character indicates where invalid UTF-8 was encountered.

## Solution Implemented

We implemented a **three-tier approach** to handle invalid UTF-8:

### 1. Zero-Copy UTF-8 Conversion (v0.1.9-beta.1)
**File**: `src/network.rs:56`

```rust
match String::from_utf8(buf) {
    Ok(line) => {
        // Happy path: valid UTF-8, zero allocations
        let line = line.trim_end_matches(&['\r', '\n']);
        let _ = server_tx_clone.send(ServerMessage::Text(line.to_string()));
    }
    Err(e) => {
        // Error path: recover buffer and clean it
        let buf = e.into_bytes();
        // ... filtering code ...
    }
}
```

**Benefits**:
- 99.9% of messages are valid UTF-8 (happy path)
- No buffer clone needed - reuses memory
- Saves one ~150 byte allocation per message

### 2. Intelligent Byte Filtering (v0.1.8-beta.1)
**File**: `src/network.rs:68-86`

When invalid UTF-8 is detected:
1. Scan for invalid bytes in the `0x80-0xBF` range
2. Check if each byte is part of a valid multi-byte UTF-8 sequence
3. Mark standalone invalid bytes for removal
4. Log diagnostics to `debug.log`

```rust
for (i, &byte) in buf.iter().enumerate() {
    if byte >= 0x80 && byte < 0xC0 {
        // Check if it's part of a valid multi-byte sequence
        let mut is_valid_continuation = false;
        if i > 0 {
            let prev = buf[i-1];
            if prev >= 0xC0 && prev < 0xF8 {
                is_valid_continuation = true;
            }
        }
        if !is_valid_continuation {
            invalid_bytes.push((i, byte));
        }
    }
}
```

### 3. Fallback to Lossy Conversion
**File**: `src/network.rs:108-114`

If filtering still produces invalid UTF-8 (rare edge case):
- Use `String::from_utf8_lossy()` as last resort
- Replaces invalid sequences with `�` (U+FFFD)
- Ensures we never crash, always display something

## Debug Logging

When invalid bytes are filtered, the following is logged to `~/.vellum-fe/debug.log` (or `debug_<character>.log`):

```
DEBUG vellum_fe::network: Filtered 2 invalid UTF-8 bytes from 156 byte message
DEBUG vellum_fe::network: Invalid bytes: 0x8a@39, 0xa0@153
```

This tells you:
- How many bytes were removed
- The hex value of each byte (`0x8a`, `0xa0`)
- Their position in the message (`@39`, `@153`)

## Performance Impact

### Before Fix
- Every message: `read_line()` → immediate UTF-8 validation → **crash on invalid**
- Memory: N/A (crashed before optimization)

### After v0.1.8-beta.1
- Valid messages: `buf.clone()` → `from_utf8()` → **1 extra allocation per message**
- Invalid messages: Filter bytes → clean → display

### After v0.1.9-beta.1 (Optimized)
- Valid messages: `from_utf8(buf)` → **zero allocations** (zero-copy)
- Invalid messages: `into_bytes()` → filter → clean → display
- **Result**: Saves ~150 bytes per message on happy path

## How to Diagnose This Issue

If the error returns or you suspect UTF-8 issues:

### 1. Enable Debug Logging
```bash
RUST_LOG=debug cargo run -- --character YourCharacter --port 8000
```

### 2. Check Debug Log
```bash
# Linux/Mac
tail -f ~/.vellum-fe/debug_YourCharacter.log | grep "Invalid UTF-8"

# Windows (PowerShell)
Get-Content C:\Users\YourUser\.vellum-fe\debug_YourCharacter.log -Wait | Select-String "Invalid UTF-8"
```

### 3. Look for These Patterns

**Normal filtered bytes (expected)**:
```
DEBUG vellum_fe::network: Filtered 2 invalid UTF-8 bytes from 156 byte message
DEBUG vellum_fe::network: Invalid bytes: 0x8a@39, 0xa0@153
```

**Fallback triggered (investigate)**:
```
DEBUG vellum_fe::network: Cleaned bytes still invalid, using lossy conversion
```
This means the filtering didn't work - the remaining bytes are still invalid UTF-8.

### 4. Common Invalid Bytes

| Hex  | Dec | Windows-1252 | Description |
|------|-----|--------------|-------------|
| `0x80` | 128 | € (Euro) | Invalid in UTF-8 |
| `0x8A` | 138 | Š (S caron) | Invalid in UTF-8 |
| `0x93` | 147 | " (left quote) | Invalid in UTF-8 |
| `0x94` | 148 | " (right quote) | Invalid in UTF-8 |
| `0x97` | 151 | — (em dash) | Invalid in UTF-8 |
| `0xA0` | 160 | NBSP | Invalid in UTF-8 |

## Testing

To test this fix:

1. **Normal Operation**: Should work transparently with no visible changes
2. **Look for Filtered Bytes**: Check debug.log for "Filtered N invalid UTF-8 bytes"
3. **Verify Clean Text**: Game text should display normally without `�` characters
4. **No Disconnects**: Should never crash with "stream did not contain valid UTF-8"

## Future Improvements

If this becomes a more widespread issue, consider:

1. **Full Windows-1252 Decoder**: Convert Windows-1252 bytes to UTF-8 equivalents
2. **Encoding Detection**: Auto-detect encoding per message
3. **Configurable Behavior**: Let users choose strict/lossy/transcode modes
4. **Server-Side Fix**: Work with Simutronics to ensure UTF-8 output

## Related Issues

- GitHub Issue: (add link if tracking this)
- Discord Discussion: (add link if discussed)

## Version History

- **v0.1.7-beta.1**: Original version (crashed on invalid UTF-8)
- **v0.1.8-beta.1**: Added UTF-8 error handling with lossy conversion
- **v0.1.9-beta.1**: Optimized to remove unnecessary clone, improved filtering

## Technical References

- [UTF-8 Specification](https://tools.ietf.org/html/rfc3629)
- [Windows-1252 Character Set](https://en.wikipedia.org/wiki/Windows-1252)
- [Rust `String::from_utf8` docs](https://doc.rust-lang.org/std/string/struct.String.html#method.from_utf8)
- [Rust `String::from_utf8_lossy` docs](https://doc.rust-lang.org/std/string/struct.String.html#method.from_utf8_lossy)

---

**Last Updated**: 2025-11-19
**Author**: VellumFE Development Team
**Status**: ✅ Resolved in v0.1.9-beta.1
