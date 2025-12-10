# Platform Issues

Platform-specific problems and solutions for Windows, macOS, and Linux.

## Windows

### Windows Terminal Issues

#### Colors Look Wrong

**Symptom**: Colors appear washed out or wrong in Windows Terminal

**Solution**:
1. Enable true color in Windows Terminal settings
2. Set color scheme to "One Half Dark" or similar
3. Add to VellumFE config:
   ```toml
   [display]
   color_mode = "truecolor"
   ```

#### Characters Display as Boxes

**Symptom**: Unicode characters show as □ or ?

**Solution**:
1. Install a font with full Unicode support:
   - Cascadia Code
   - JetBrains Mono
   - Nerd Font variants

2. Set the font in Windows Terminal settings

3. Or disable Unicode in VellumFE:
   ```toml
   [display]
   unicode = false
   ```

#### Slow Startup

**Symptom**: VellumFE takes several seconds to start

**Solution**:
1. Check antivirus exclusions - add VellumFE executable
2. Disable Windows Defender real-time scanning for config directory
3. Run from SSD rather than HDD

### PowerShell Issues

#### Keybinds Not Working

**Symptom**: Certain key combinations don't register

**Cause**: PowerShell intercepts some keys

**Solution**:
1. Use Windows Terminal instead of PowerShell directly
2. Or disable PSReadLine:
   ```powershell
   Remove-Module PSReadLine
   ```

#### Copy/Paste Issues

**Symptom**: Can't paste into VellumFE

**Solution**:
1. Use `Ctrl+Shift+V` instead of `Ctrl+V`
2. Or enable "Use Ctrl+Shift+C/V as Copy/Paste" in terminal settings

### WSL Issues

#### Connection to Lich Fails

**Symptom**: Can't connect to Lich running in Windows from WSL

**Solution**:
```bash
# In WSL, use Windows host IP
vellum-fe --host $(cat /etc/resolv.conf | grep nameserver | awk '{print $2}') --port 8000

# Or use localhost forwarding
vellum-fe --host localhost --port 8000
```

#### Performance Issues

**Symptom**: Slow or laggy in WSL

**Solution**:
1. Use WSL2 (not WSL1)
2. Store files in Linux filesystem, not `/mnt/c/`
3. Increase WSL memory allocation in `.wslconfig`

### OpenSSL on Windows

#### "Can't find OpenSSL"

**Symptom**: Direct mode fails with OpenSSL errors

**Solution**:
1. Install via vcpkg:
   ```cmd
   vcpkg install openssl:x64-windows
   ```

2. Set environment variable:
   ```cmd
   set VCPKG_ROOT=C:\path\to\vcpkg
   ```

3. Rebuild VellumFE

## macOS

### Terminal.app Issues

#### Limited Colors

**Symptom**: Only 256 colors, not full truecolor

**Solution**:
1. Use iTerm2 or Alacritty instead of Terminal.app
2. Or configure for 256 colors:
   ```toml
   [display]
   color_mode = "256"
   ```

#### Function Keys Don't Work

**Symptom**: F1-F12 keys trigger macOS features instead

**Solution**:
1. System Preferences → Keyboard → "Use F1, F2, etc. keys as standard function keys"
2. Or use modifier:
   ```toml
   [keybinds."fn+f1"]
   macro = "look"
   ```

### iTerm2 Issues

#### Mouse Not Working

**Symptom**: Can't click on widgets or compass

**Solution**:
1. iTerm2 → Preferences → Profiles → Terminal
2. Enable "Report mouse clicks"
3. Enable "Report mouse wheel events"

#### Scrollback Conflict

**Symptom**: Page Up/Down scrolls iTerm instead of VellumFE

**Solution**:
1. iTerm2 → Preferences → Keys → Key Bindings
2. Remove or remap Page Up/Down
3. Or use VellumFE scrollback keys:
   ```toml
   [keybinds."shift+up"]
   action = "scroll_up"
   ```

### Security Issues

#### "Cannot be opened because the developer cannot be verified"

**Symptom**: macOS Gatekeeper blocks VellumFE

**Solution**:
```bash
# Remove quarantine attribute
xattr -d com.apple.quarantine /path/to/vellum-fe

# Or allow in System Preferences → Security & Privacy
```

#### "Permission denied" for Audio

**Symptom**: TTS features fail

**Solution**:
1. System Preferences → Security & Privacy → Privacy → Microphone
2. Add Terminal/iTerm2
3. Also check Accessibility permissions

### Apple Silicon (M1/M2)

#### Rosetta Performance

**Symptom**: VellumFE slow on Apple Silicon

**Cause**: Running x86 binary through Rosetta

**Solution**:
- Download ARM64/aarch64 build if available
- Or build from source:
  ```bash
  rustup target add aarch64-apple-darwin
  cargo build --release --target aarch64-apple-darwin
  ```

## Linux

### Distribution-Specific Issues

#### Ubuntu/Debian

**Missing Libraries**:
```bash
# Install required libraries
sudo apt install libssl-dev pkg-config

# For audio (TTS)
sudo apt install libasound2-dev
```

**Wayland Issues**:
```bash
# If running Wayland, may need XWayland for some features
sudo apt install xwayland
```

#### Fedora/RHEL

**Missing Libraries**:
```bash
sudo dnf install openssl-devel pkg-config

# For audio
sudo dnf install alsa-lib-devel
```

#### Arch Linux

**Missing Libraries**:
```bash
sudo pacman -S openssl pkgconf

# For audio
sudo pacman -S alsa-lib
```

### Terminal Emulator Issues

#### Alacritty

**Font Rendering**:
```yaml
# ~/.config/alacritty/alacritty.yml
font:
  normal:
    family: "JetBrains Mono"
  size: 11
```

**Key Bindings Conflict**:
- Check `~/.config/alacritty/alacritty.yml` for conflicting bindings

#### Kitty

**Graphics Protocol**:
```conf
# ~/.config/kitty/kitty.conf
# Disable if causing issues
allow_remote_control no
```

**Unicode**:
```conf
symbol_map U+E000-U+F8FF Symbols Nerd Font
```

#### GNOME Terminal

**True Color**:
```bash
# Verify support
echo $COLORTERM  # Should show "truecolor"

# If not, set in .bashrc
export COLORTERM=truecolor
```

### X11 vs Wayland

#### Clipboard Issues

**Symptom**: Copy/paste doesn't work

**X11 Solution**:
```bash
# Install xclip
sudo apt install xclip
```

**Wayland Solution**:
```bash
# Install wl-clipboard
sudo apt install wl-clipboard

# Set in config
[clipboard]
backend = "wayland"  # or "x11"
```

### Audio Issues (TTS)

#### PulseAudio

**Symptom**: TTS doesn't play sound

**Solution**:
```bash
# Check PulseAudio is running
pulseaudio --check

# Restart if needed
pulseaudio --kill
pulseaudio --start
```

#### PipeWire

**Symptom**: Audio issues on modern systems

**Solution**:
```bash
# Install PipeWire PulseAudio compatibility
sudo apt install pipewire-pulse

# Restart
systemctl --user restart pipewire pipewire-pulse
```

### Permission Issues

#### "Operation not permitted"

**Symptom**: Can't write to config directory

**Solution**:
```bash
# Check ownership
ls -la ~/.vellum-fe

# Fix if needed
sudo chown -R $USER:$USER ~/.vellum-fe
chmod 755 ~/.vellum-fe
chmod 644 ~/.vellum-fe/*.toml
```

#### SELinux Blocking

**Symptom**: Works as root but not as user (Fedora/RHEL)

**Solution**:
```bash
# Check if SELinux is blocking
sudo ausearch -m avc -ts recent

# Create policy exception if needed
# (Consult SELinux documentation)
```

## Cross-Platform Issues

### Locale/Encoding

#### "Invalid UTF-8"

**Symptom**: Errors about encoding

**Solution**:
```bash
# Check locale
locale

# Set UTF-8 locale
export LC_ALL=en_US.UTF-8
export LANG=en_US.UTF-8
```

### Time Zone Issues

#### Timestamps Wrong

**Symptom**: Log timestamps incorrect

**Solution**:
```bash
# Check timezone
date

# Set if needed
export TZ="America/New_York"
```

Or configure:
```toml
[logging]
use_local_time = true
```

### Path Issues

#### "File not found" for Config

**Symptom**: Can't find config files

**Solution**:
- Use explicit paths:
  ```bash
  vellum-fe --config /full/path/to/config.toml
  ```
- Check `$HOME` is set correctly
- Verify `~` expansion works in your shell

## See Also

- [Installation](../getting-started/installation.md) - Setup by platform
- [Common Errors](./common-errors.md) - Error messages
- [Display Issues](./display-issues.md) - Visual problems

