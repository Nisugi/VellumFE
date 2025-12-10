# First Launch

This guide walks you through connecting to GemStone IV for the first time.

## Choose Your Connection Mode

VellumFE supports two connection methods:

| Mode | Best For | Requirements |
|------|----------|--------------|
| **Lich Proxy** | Script users, most players | Lich running |
| **Direct** | Standalone use, lightweight | Account credentials |

---

## Option A: Lich Proxy Mode

### Step 1: Start Lich

Launch Lich with your character. If you're new to Lich, see the [Lich documentation](https://lichproject.org/).

```bash
# Example Lich launch (varies by setup)
ruby lich.rb --login CharacterName
```

Lich will start listening on a port (default: 8000).

### Step 2: Launch VellumFE

```bash
# Connect to Lich on default port
vellum-fe

# Or specify a different port
vellum-fe --port 8001
```

### Step 3: Verify Connection

You should see:
1. VellumFE window appears
2. Game output starts flowing
3. Prompt (">") is visible

If the connection fails:
- Ensure Lich is running and logged in
- Check the port number matches
- See [Connection Troubleshooting](../troubleshooting/connection-issues.md)

---

## Option B: Direct Mode

Direct mode connects without Lich, authenticating directly with Simutronics.

### Step 1: Launch with Credentials

```bash
# Minimal command (will prompt for password)
vellum-fe --direct --account YOUR_ACCOUNT --character CharName

# Specify game world
vellum-fe --direct --account YOUR_ACCOUNT --character CharName --game prime
```

### Game Worlds

| World | Flag | Description |
|-------|------|-------------|
| Prime | `--game prime` | Main GemStone IV server |
| Platinum | `--game platinum` | Premium subscription server |
| Shattered | `--game shattered` | Test/development server |

### Step 2: Enter Password

When prompted, enter your account password. The password is not echoed for security.

```
Password for account YOUR_ACCOUNT: ********
```

### Step 3: Verify Connection

VellumFE will:
1. Authenticate with eAccess servers
2. Download/verify the server certificate (first time only)
3. Connect to the game server
4. Display game output

---

## Initial Configuration

On first launch, VellumFE creates default configuration files:

```
~/.vellum-fe/
├── config.toml      # Created with defaults
├── layout.toml      # Default window layout
├── keybinds.toml    # Default keybindings
├── highlights.toml  # Default highlights
└── colors.toml      # Default color theme
```

### Character Profiles

To use per-character settings:

```bash
vellum-fe --character CharName
```

This loads settings from `~/.vellum-fe/profiles/CharName/` if they exist, falling back to defaults.

---

## Essential First Steps

Once connected, try these:

### 1. Test Commands
Type a game command and press Enter:
```
look
```

### 2. Test Scrolling
Use Page Up/Down or scroll wheel to navigate history.

### 3. Open the Menu
Press `Ctrl+M` to open the main menu. Navigate with arrow keys.

### 4. Check Keybinds
Press `Ctrl+?` to view current keybindings.

---

## First Launch Checklist

- [ ] Connected successfully
- [ ] Game output appears
- [ ] Commands work
- [ ] Colors display correctly
- [ ] Menu opens with Ctrl+M

---

## Common First-Launch Issues

### "Connection refused"
- **Lich mode**: Ensure Lich is running and logged in
- **Direct mode**: Check internet connection

### No output appears
- Check that you're connected to the right port
- Ensure Lich/game is actually sending data

### Colors look wrong
- Set `COLORTERM=truecolor` in your terminal
- Use Windows Terminal (not CMD) on Windows

### Password not accepted (Direct mode)
- Verify account name and password
- Try logging in via the official client first
- Delete `~/.vellum-fe/simu.pem` and retry

---

## Next Steps

Now that you're connected, take a [Quick Tour](./quick-tour.md) to learn the essential controls.

---

## See Also

- [Quick Tour](./quick-tour.md) - Essential keyboard shortcuts
- [Configuration](../configuration/README.md) - Customize your setup
- [Connection Troubleshooting](../troubleshooting/connection-issues.md) - Detailed connection help
