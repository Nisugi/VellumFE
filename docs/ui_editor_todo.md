## UI Editor Modularization TODO

### 0. Baseline
- [ ] Capture before/after screenshots of color editor + keybind/highlight editors for visual regression comparisons.
- [ ] Document current keyboard shortcuts and mouse interactions used in all editors (color, keybind, highlight, window, settings).

### 1. Shared Theme & Styling
- [ ] Extract palette, typography, spacing, and border styles from `ColorForm`/`ColorPaletteBrowser` into `ui::theme`.
- [ ] Replace hard-coded colors in all existing editors with references to `ui::theme`.
- [ ] Centralize button and checkbox rendering styles; create helper functions in `ui::theme`.

### 2. Popup Infrastructure
- [ ] Implement `PopupState` struct (position, drag offsets, bounds checks, hit-testing).
- [ ] Add `PopupFrame` renderer to draw borders/title/status area.
- [ ] Migrate `ColorForm` to new popup abstraction; verify drag + clamp behavior.
- [ ] Roll out `PopupState` to `SpellColorForm`, `ColorPaletteBrowser`, `SpellColorBrowser`, `KeybindForm`, `KeybindBrowser`, `HighlightForm`, `HighlightBrowser`, `WindowEditor`, `SettingsEditor`.

### 3. Form Framework
- [ ] Design `FormField` descriptors (text, checkbox, select, action button, spacer).
- [ ] Build `FormState` for focus cycling, validation hooks, and CTA dispatch.
- [ ] Create reusable renderers: `render_form`, `render_buttons`, `render_checkboxes`.
- [ ] Port `ColorForm` + `SpellColorForm` to descriptor-based framework.
- [ ] Port `KeybindForm` (ensure action dropdown + macro text behavior respected).
- [ ] Port `HighlightForm` (regex validation + error display).
- [ ] Hook analytics/tests for per-field validation to prevent regressions.

### 4. Browser/List Framework
- [ ] Draft `BrowserState<T>` with filtering, paging, and section headers.
- [ ] Define trait (`BrowserView`) for rendering list rows + detail columns.
- [ ] Convert `ColorPaletteBrowser` and `SpellColorBrowser`.
- [ ] Convert `KeybindBrowser` (ensure filter-by-key + action grouping supported).
- [ ] Convert `HighlightBrowser` (category sections, status bar messages).

### 5. Window Editor Decomposition
- [ ] Break `window_editor.rs` into submodules: `selection`, `widget_picker`, `template_picker`, `tab_editor`, `indicator_editor`, `layout_preview`.
- [ ] Migrate per-submodule UI to shared popup/form/browser components.
- [ ] Ensure tab editor uses `FormState` for name/stream inputs and `BrowserState` for tab list.
- [ ] Replace custom indicator picker with shared dropdown/list components.
- [ ] Write integration tests for window save/cancel flows.

### 6. Settings Editor Modernization
- [ ] Audit settings editor structure; map each section to shared components.
- [ ] Implement multi-section layout using shared form framework.
- [ ] Add regression tests for loading/saving settings.

### 7. Testing & Tooling
- [ ] Add unit tests for `PopupState`, `FormState`, and `BrowserState` behavior (focus cycle, bounds clamp, filter results).
- [ ] Create scripted snapshot tests (text dumps) for representative editors.
- [ ] Integrate lint/clippy rules enforcing use of shared components where applicable.
- [ ] Update developer documentation (`docs/ui/README.md`) with usage examples and module diagrams.

### 8. Migration Tracking
- [ ] Maintain checklist of files converted to shared framework (include dates + reviewer).
- [ ] Add CI guard ensuring new editors implement `EditorWidget` trait + shared theme.
- [ ] Plan follow-up session to evaluate performance metrics post-refactor.

