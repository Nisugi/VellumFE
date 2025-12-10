# Tutorials

Step-by-step guides for common VellumFE setups and workflows.

## Tutorial Philosophy

These tutorials guide you through complete workflows, explaining not just *what* to do but *why* each choice matters. Each tutorial builds a functional setup from scratch.

## Available Tutorials

### [Your First Layout](./your-first-layout.md)

Start here! Create a basic but functional layout:

- Understanding the coordinate system
- Placing essential widgets
- Testing and iteration
- Saving and loading

**Time**: 30 minutes | **Difficulty**: Beginner

### [Hunting Setup](./hunting-setup.md)

Optimized layout for combat and hunting:

- Combat-focused widget arrangement
- Health/mana monitoring
- Quick action macros
- Creature targeting

**Time**: 45 minutes | **Difficulty**: Intermediate

### [Merchant Setup](./merchant-setup.md)

Layout for trading and crafting:

- Inventory management
- Transaction logging
- Crafting timers
- Trade window optimization

**Time**: 30 minutes | **Difficulty**: Intermediate

### [Roleplay Setup](./roleplay-setup.md)

Immersive layout for roleplaying:

- Expanded text areas
- Chat organization
- Minimal HUD elements
- Atmosphere preservation

**Time**: 30 minutes | **Difficulty**: Beginner

### [Minimal Layout](./minimal-layout.md)

Clean, distraction-free interface:

- Essential widgets only
- Maximum text space
- Keyboard-centric design
- Low resource usage

**Time**: 20 minutes | **Difficulty**: Beginner

### [Accessibility Setup](./accessibility.md)

Screen reader and high contrast configurations:

- TTS integration
- High contrast themes
- Keyboard navigation
- Large text options

**Time**: 45 minutes | **Difficulty**: Intermediate

## Tutorial Structure

Each tutorial follows the same pattern:

1. **Goal** - What we're building
2. **Prerequisites** - What you need first
3. **Step-by-Step** - Detailed instructions
4. **Configuration** - Complete config files
5. **Testing** - Verification steps
6. **Customization** - Ways to adapt it
7. **Troubleshooting** - Common issues

## Before You Start

### Prerequisites

1. VellumFE installed and running
2. Basic familiarity with TOML syntax
3. A text editor
4. Game account for testing

### Backup First

Before modifying configurations:

```bash
# Backup your configs
cp -r ~/.vellum-fe ~/.vellum-fe.backup
```

### Configuration Location

All config files live in `~/.vellum-fe/`:

```
~/.vellum-fe/
├── config.toml      # Main settings
├── layout.toml      # Widget positions
├── colors.toml      # Theme colors
├── highlights.toml  # Text patterns
└── keybinds.toml    # Key mappings
```

## Learning Path

### Complete Beginner

1. [Your First Layout](./your-first-layout.md)
2. [Minimal Layout](./minimal-layout.md)
3. [Roleplay Setup](./roleplay-setup.md)

### Combat Focus

1. [Your First Layout](./your-first-layout.md)
2. [Hunting Setup](./hunting-setup.md)

### Accessibility Needs

1. [Accessibility Setup](./accessibility.md)
2. [Minimal Layout](./minimal-layout.md)

### Trading/Crafting

1. [Your First Layout](./your-first-layout.md)
2. [Merchant Setup](./merchant-setup.md)

## Tutorial Tips

### Take Your Time

Don't rush through tutorials. Understanding each step helps you customize later.

### Experiment

Tutorials provide starting points. Try variations to find what works for you.

### Ask Questions

If something's unclear:
- Check [Troubleshooting](../troubleshooting/README.md)
- Search existing issues
- Ask the community

### Share Your Setups

Created something cool? Consider sharing it with the community!

## See Also

- [Cookbook](../cookbook/README.md) - Quick recipes for specific tasks
- [Configuration Reference](../configuration/README.md) - Complete config documentation
- [Widget Reference](../widgets/README.md) - All widget types

