# .uicolors Implementation Plan

## Status: IN PROGRESS

### Completed ✅
1. Created `ColorConfig` struct with `presets`, `prompt_colors`, `ui: UiColors`, `spell_colors`
2. Created `defaults/colors.toml` with all color defaults
3. Embedded `DEFAULT_COLORS` constant
4. Updated `extract_defaults()` to extract colors.toml to profile directory
5. Added `Config::colors_path()` helper

### Next Steps

#### 1. Add ColorConfig Loading to Config
- Add `colors: ColorConfig` field to `Config` struct
- Load colors.toml in `Config::load_with_options()`
- Add `ColorConfig::load()` and `ColorConfig::save()` methods
- Update all color references to use `config.colors.ui.border_color` etc.

#### 2. Update Config Struct (src/config.rs)
Remove these fields from `Config`:
- `presets: HashMap<String, PresetColor>` → move to `colors.presets`
- `spell_colors: Vec<SpellColorRange>` → move to `colors.spell_colors`

Remove these fields from `UiConfig`:
- `command_echo_color` → move to `colors.ui.command_echo_color`
- `border_color` → move to `colors.ui.border_color`
- `focused_border_color` → move to `colors.ui.focused_border_color`
- `text_color` → move to `colors.ui.text_color`
- `background_color` → move to `colors.ui.background_color`
- `prompt_colors` → move to `colors.prompt_colors`
- `selection_bg_color` → move to `colors.ui.selection_bg_color`

#### 3. Update All Color References
Search and replace throughout codebase:
- `self.config.presets` → `self.config.colors.presets`
- `self.config.spell_colors` → `self.config.colors.spell_colors`
- `self.config.ui.command_echo_color` → `self.config.colors.ui.command_echo_color`
- `self.config.ui.border_color` → `self.config.colors.ui.border_color`
- `self.config.ui.focused_border_color` → `self.config.colors.ui.focused_border_color`
- `self.config.ui.text_color` → `self.config.colors.ui.text_color`
- `self.config.ui.background_color` → `self.config.colors.ui.background_color`
- `self.config.ui.prompt_colors` → `self.config.colors.prompt_colors`
- `self.config.ui.selection_bg_color` → `self.config.colors.ui.selection_bg_color`

#### 4. Create UIColorsBrowser Widget (src/ui/ui_colors_browser.rs)

**Spec:**
- Window size: 52x20
- Categories: Presets, Prompt Colors, UI Colors
- Navigation: Tab/Shift+Tab AND Up/Down arrows
- Edit: Enter OR Space to edit selected item
- Save: Ctrl+S (saves to colors.toml)
- Cancel: Esc (closes without saving)
- NO delete functionality

**Structure:**
```rust
pub struct UIColorsBrowser {
    // Popup position
    popup_x: u16,
    popup_y: u16,
    dragging: bool,
    drag_offset_x: u16,
    drag_offset_y: u16,

    // Categories and items
    categories: Vec<ColorCategory>,
    focused_item: usize,
    scroll_offset: usize,

    // Editing state
    editing: Option<usize>,  // Index of item being edited
    edit_buffer: String,

    // Color config reference
    colors: ColorConfig,
}

pub enum ColorCategory {
    Presets { items: Vec<(String, PresetColor)> },
    PromptColors { items: Vec<PromptColor> },
    UiColors { items: Vec<(&'static str, String)> },  // (label, color_value)
}

pub enum UIColorsBrowserResult {
    Save { colors: ColorConfig },
    Cancel,
}
```

**Rendering:**
- Black background with region clear (per style guide)
- Cyan single border
- Title: " UI Colors (drag to move) "
- Category headers in yellow/bold
- Selected item in gold
- Editing item shows dark maroon input area
- Color preview `[#RRGGBB]` boxes

#### 5. Add .uicolors Command (src/app.rs)

In `handle_dot_command()`:
```rust
"uicolors" | "colors" => {
    self.ui_colors_browser = Some(UIColorsBrowser::new(self.config.colors.clone()));
    self.input_mode = InputMode::UIColorsBrowser;
    self.add_system_message("UI Colors browser opened");
}
```

Add handling in event loop:
```rust
InputMode::UIColorsBrowser => {
    if let Some(ref mut browser) = self.ui_colors_browser {
        if let Some(result) = browser.handle_key(key_event) {
            match result {
                UIColorsBrowserResult::Save { colors } => {
                    self.config.colors = colors;
                    if let Err(e) = self.config.save_colors() {
                        self.add_system_message(&format!("Failed to save colors: {}", e));
                    } else {
                        self.add_system_message("Colors saved to colors.toml");
                    }
                }
                UIColorsBrowserResult::Cancel => {}
            }
            self.ui_colors_browser = None;
            self.input_mode = InputMode::Normal;
        }
    }
}
```

#### 6. Update .menu System

In `populate_menu_categories()`:
```rust
self.menu_categories.insert(
    "colors".to_string(),
    vec![
        MenuItem { text: "Browse highlights".to_string(), command: ".highlights".to_string() },
        MenuItem { text: "Add highlight".to_string(), command: ".addhl".to_string() },
        MenuItem { text: "Browse spell colors".to_string(), command: ".spellcolors".to_string() },
        MenuItem { text: "Add spell color".to_string(), command: ".addspellcolor".to_string() },
        MenuItem { text: "Browse UI colors".to_string(), command: ".uicolors".to_string() },  // NEW
    ]
);
```

#### 7. Remove from .settings Editor (src/app.rs)

Remove these SettingItem entries:
- `command_echo_color`
- `border_color`
- `focused_border_color`
- `text_color`
- `border_style` (keep this - it's not a color)
- `background_color`
- `selection_bg_color`

Remove from `validate_setting_value()`:
- Color validation for above fields

Remove from `save_setting_value()`:
- Save handlers for above fields

#### 8. Testing Checklist

- [ ] Build compiles without errors
- [ ] colors.toml extracts to `~/.vellum-fe/{character}/colors.toml`
- [ ] `.uicolors` command opens browser
- [ ] Browser shows all preset colors
- [ ] Browser shows all prompt colors
- [ ] Browser shows all UI colors
- [ ] Tab/Shift+Tab navigates items
- [ ] Up/Down arrows navigate items
- [ ] Enter/Space starts editing
- [ ] Editing shows dark maroon input area
- [ ] Color preview boxes display correctly
- [ ] Ctrl+S saves changes to colors.toml
- [ ] Esc cancels without saving
- [ ] Color changes apply immediately after save
- [ ] .menu has "Browse UI colors" option
- [ ] Removed colors no longer appear in .settings

## File Structure

```
~/.vellum-fe/
└── {character}/
    ├── config.toml      # Non-color settings
    ├── colors.toml      # All color settings (NEW)
    ├── layout.toml      # Auto-saved layout
    ├── history.txt
    └── debug.log
```

## Implementation Priority

1. **HIGH**: Update Config to load ColorConfig
2. **HIGH**: Update all color reference paths
3. **MEDIUM**: Create UIColorsBrowser widget
4. **MEDIUM**: Add .uicolors command
5. **LOW**: Remove colors from .settings
6. **LOW**: Add to .menu system
