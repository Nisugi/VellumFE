# Cookbook

Quick recipes for common VellumFE customizations and configurations.

## Philosophy

The cookbook provides focused, copy-paste-ready solutions for specific tasks. Each recipe is self-contained and can be used independently.

## Recipe Format

Each recipe includes:
1. **Goal** - What you'll achieve
2. **Configuration** - Complete TOML to copy
3. **Explanation** - How it works
4. **Variations** - Alternative approaches

## Available Recipes

### Layout Recipes

- [Split Main Window](./split-main-window.md) - Divide main text area into multiple panels
- [Floating Compass](./floating-compass.md) - Overlay compass on main window
- [Transparent Overlays](./transparent-overlays.md) - Semi-transparent widgets

### Feedback Recipes

- [Combat Alerts](./combat-alerts.md) - Visual and audio combat notifications
- [Custom Status Bar](./custom-status-bar.md) - Personalized status display

### Organization Recipes

- [Tabbed Channels](./tabbed-channels.md) - Organize communications in tabs

## Quick Reference

### Common Tasks

| Want to... | Recipe |
|------------|--------|
| Split main window | [Split Main Window](./split-main-window.md) |
| Overlay compass | [Floating Compass](./floating-compass.md) |
| Combat sounds | [Combat Alerts](./combat-alerts.md) |
| Organize chat | [Tabbed Channels](./tabbed-channels.md) |
| Custom HUD | [Custom Status Bar](./custom-status-bar.md) |

### By Difficulty

**Beginner**:
- Combat Alerts
- Custom Status Bar

**Intermediate**:
- Split Main Window
- Tabbed Channels
- Floating Compass

**Advanced**:
- Transparent Overlays

## Using Recipes

### Copy and Modify

1. Find the recipe for your goal
2. Copy the configuration
3. Paste into your config file
4. Adjust values for your preferences

### Combining Recipes

Recipes are designed to work together. To combine:
1. Copy configurations from each recipe
2. Adjust positions to prevent overlap
3. Ensure no naming conflicts

### Testing

After applying a recipe:
1. Reload configuration (`.reload` or restart)
2. Verify the feature works
3. Adjust as needed

## Contributing Recipes

Have a useful configuration? Consider contributing:
1. Document it following the recipe format
2. Test thoroughly
3. Submit via GitHub

## See Also

- [Tutorials](../tutorials/README.md) - Step-by-step guides
- [Configuration](../configuration/README.md) - Config reference
- [Customization](../customization/README.md) - Detailed customization

