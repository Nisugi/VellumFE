# CONFIG MODULE

**Generated:** 2025-12-29T19:53:00Z
**Parent:** ./AGENTS.md

## OVERVIEW

Configuration system handling TOML-based settings for keybinds, highlights, and menu bindings with validation and auto-fixing.

## STRUCTURE

```
src/config/
├── keybinds.rs        # App/menu keybinds with parsing, 1,761 lines
├── highlights.rs      # Text highlighting patterns, 821 lines  
└── menu_keybind_validator.rs  # Validation logic, 285 lines
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Add new keybind action | `keybinds.rs` | Update KeyAction enum + from_str() |
| Modify highlight patterns | `highlights.rs` | Update HighlightPattern struct |
| Parse key strings | `keybinds.rs::parse_key_string()` | Converts "ctrl+f" to KeyCode |
| Load/save configs | `Config` impl methods | Merge global + character configs |
| Validate menu bindings | `menu_keybind_validator.rs` | Check duplicates/critical missing |

## CODE MAP

| Symbol | Type | Location | Refs | Role |
|--------|------|----------|-------|------|
| KeyBindAction | Enum | `keybinds.rs` | High | Action or Macro binding variants |
| KeyAction | Enum | `keybinds.rs` | High | All possible actions (56 variants) |
| MenuKeybinds | Struct | `keybinds.rs` | High | Menu-specific keybind configuration |
| HighlightPattern | Struct | `highlights.rs` | Medium | Regex/sound/redirect patterns |
| EventPattern | Struct | `highlights.rs` | Medium | Game event timer patterns |
| ValidationResult | Struct | `menu_keybind_validator.rs` | Low | Validation errors/warnings |

## CONVENTIONS

- **TOML serialization**: Uses serde with skip_serializing_if for defaults
- **Global overrides**: Character configs override global configs via HashMap::extend()
- **Key parsing**: Supports "ctrl+alt+shift+f5" format, case-insensitive
- **Validation**: Critical bindings (cancel, navigate) must not be empty
- **Performance**: Compiled regex cached in HighlightPattern.compiled_regex

## ANTI-PATTERNS (THIS MODULE)

- **Empty keybind strings**: Critical menu actions cannot be empty - causes validation errors
- **Duplicate bindings**: Same key assigned to multiple menu actions triggers warnings
- **Regex compilation failure**: Invalid regex patterns logged as warnings, not errors
- **Hardcoded defaults**: Avoid changing defaults - breaks user configurations

## UNIQUE STYLES

- **Dual keybind systems**: Separate AppKeybinds (global) and MenuKeybinds (context-specific)
- **Fast parse optimization**: Aho-Corasick for literal "|" separated patterns
- **Auto-fix validation**: Can restore defaults for missing critical bindings
- **Redirect modes**: Lines can be redirected only or copied to other windows
- **Numpad movement**: Default macros for num_0-9 directional movement