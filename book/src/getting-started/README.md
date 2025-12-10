# Getting Started

Welcome to VellumFE! This section will guide you from installation to your first game session.

## Overview

Getting VellumFE running involves three steps:

1. **[Installation](./installation.md)** - Download or build VellumFE for your platform
2. **[First Launch](./first-launch.md)** - Connect to GemStone IV
3. **[Quick Tour](./quick-tour.md)** - Learn the essential controls

## Prerequisites

Before you begin, ensure you have:

- **A GemStone IV Account**: [Create one at play.net](https://www.play.net/gs4/)
- **A Modern Terminal**: Windows Terminal, iTerm2, or any terminal with 256+ color support
- **Optional: Lich**: For scripting support, [install Lich](https://lichproject.org/)

## Connection Modes

VellumFE offers two ways to connect:

### Lich Proxy Mode (Recommended for Script Users)
```bash
# Start Lich with your character, then:
vellum-fe --port 8000
```
- Full Lich script compatibility
- Scripting and automation support
- Most users choose this option

### Direct Mode (Standalone)
```bash
vellum-fe --direct --account YOUR_ACCOUNT --character CharName --game prime
```
- No Lich dependency
- Authenticates directly with eAccess
- Ideal for lightweight setups

## What's Next?

| Page | Description |
|------|-------------|
| [Installation](./installation.md) | Platform-specific setup instructions |
| [First Launch](./first-launch.md) | Connecting and initial configuration |
| [Quick Tour](./quick-tour.md) | Essential keyboard shortcuts and navigation |
| [Upgrading](./upgrading.md) | Updating to new versions |

---

*Ready to install? Continue to [Installation](./installation.md).*
