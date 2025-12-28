# VellumFE v0.2.0-beta.11

A complete architecture rewrite with **63 commits** of improvements since v0.1.9-beta.5.

## Highlights

### Direct eAccess Connection
Connect directly to GemStone IV without Lich proxy:
```bash
vellum-fe --direct --account ACCOUNT --password PASS --game prime --character NAME
```

### New Widgets
- **Spells Window** - Clickable spells with cmdlist.xml integration
- **Container Window** - Display bag/container contents
- **Experience Window** - DragonRealms skill tracking
- **Targets/Players Widgets** - Room occupant display with click targeting

### Major Features
- **Theme System** - Unified colors.toml with preset inheritance
- **Universal Highlighting** - All text widgets now support highlights
- **Sound Queue** - Highlights trigger sounds exactly when matched
- **Migration Tools** - Import layouts from old VellumFE versions

### DragonRealms Support
- Game codes: dr, drplatinum, drfallen, drtest
- Concentration progress bar (replaces mana)
- Experience component tracking

### Quality of Life
- `.reload highlights` - Test changes without restart
- `.lockwindows` / `.unlockwindows` - Prevent accidental moves
- Right-click context menu on window borders
- Tab cycles focused window for scrolling
- `--nosound` flag for headless systems
- `--color-mode slot` for 256-color terminals

## Bug Fixes
- Fixed `.addwindow` not receiving updates (spells/inventory now work immediately)
- Fixed layout auto-scaling issues (windows use exact positions now)
- Fixed highlight sound duplication
- Fixed consecutive prompt display
- Fixed focus exclude list deadlock

## Statistics
- **2,300+ tests** passing
- **19 widget types** supported
- **~600 spell abbreviations** database

## Upgrade Notes

Backwards compatible with v0.1.9-beta.5 configs.

### Migrating from v0.1.x Layouts
If you have layouts from older VellumFE versions:
```bash
# Preview what would change (dry run)
vellum-fe migrate-layout --src ~/.vellum-fe/ --dry-run -v

# Migrate in place
vellum-fe migrate-layout --src ~/.vellum-fe/

# Or migrate to a new directory
vellum-fe migrate-layout --src ~/.vellum-fe-old/ --out ~/.vellum-fe/
```

### Optional Config Changes
- `sound.disabled = true` → `sound.enabled = false`
- New `[streams]` section for custom routing
- Check `~/.vellum-fe/global/templates/` for documented config examples

---

**Full changelog**: [CHANGELOG.md](CHANGELOG.md) | [Detailed](CHANGELOG_DETAILED.md)
