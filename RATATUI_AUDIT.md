# Ratatui Feature Audit - VellumFE

## Executive Summary
This audit identifies features we've reimplemented that already exist in Ratatui, causing bugs (text artifacts) and unnecessary complexity.

---

## Critical Issues - Reinvented Wheels

### 1. ⚠️ **TextWindow: Custom Text Wrapping**
**What we did:** Implemented custom `wrap_styled_spans()` function that manually wraps text
**What Ratatui has:** `Paragraph::wrap(Wrap { trim: bool })`
**Impact:**
- **CAUSES TEXT ARTIFACTS** - Our wrapping conflicts with Paragraph's rendering
- Adds ~200 lines of complex wrapping logic
- Requires maintaining `wrapped_lines` cache
- Requires `rewrap_all()` when window resizes

**Ratatui Feature:**
```rust
Paragraph::new(text)
    .wrap(Wrap { trim: true })  // Handles wrapping automatically
```

---

### 2. ⚠️ **TextWindow: Manual Scrolling**
**What we did:** Implemented custom scrolling with `scroll_offset` and `scroll_position`
**What Ratatui has:** `Paragraph::scroll((y, x))`
**Impact:**
- Adds complexity tracking scroll state
- Manual calculation of which lines to show
- ~100 lines of scroll management code

**Ratatui Feature:**
```rust
Paragraph::new(text)
    .scroll((self.scroll_offset, 0))  // Built-in scrolling
```

---

### 3. ⚠️ **ScrollableContainer: Custom Scrolling**
**What we did:** Implemented manual `scroll_offset` tracking in scrollable_container.rs
**What Ratatui has:** `List` widget with built-in scrolling via `ListState`
**Impact:**
- Reimplements scrolling logic
- Should use `List` widget with `ListState::selected()`

**Ratatui Feature:**
```rust
let mut list_state = ListState::default();
list_state.select(Some(selected_index));
List::new(items)
    .highlight_symbol(">> ")
    .render(area, buf, &mut list_state);  // Handles scrolling automatically
```

---

### 4. ⚠️ **ScrollableContainer: Manual Item Rendering**
**What we did:** Custom rendering of items with progress bars
**What Ratatui has:** `List` widget that renders items
**Impact:**
- Could use `List` with custom `ListItem` rendering
- List handles selection, scrolling, highlighting automatically

---

### 5. ⚠️ **TextWindow: Line Padding for Artifact Prevention**
**What we did:** Added manual padding of lines with spaces
**What Ratatui handles:** Paragraph clears its area when using native wrapping
**Impact:**
- Band-aid fix for artifact problem caused by custom wrapping
- Unnecessary once we use Paragraph's native wrap

---

## Medium Priority Issues

### 6. **TabbedTextWindow: Multiple TextWindows**
**What we did:** Created tabbed window that contains multiple TextWindow instances
**Potential improvement:** Could potentially use Ratatui's `Tabs` widget combined with a single Paragraph
**Impact:**
- Current approach is reasonable
- Could simplify by using single Paragraph with tab switching
- Lower priority

---

### 7. **PopupMenu: Custom Menu Rendering**
**What we did:** Custom popup menu widget
**What Ratatui has:** `List` widget with selection
**Impact:**
- PopupMenu is functionally similar to a List
- Could potentially refactor to use List widget
- Current implementation works, but adds maintenance burden

---

## Low Priority / Acceptable

### 8. **Progress Bars, Countdown, Indicators**
**Status:** ✅ Acceptable
- These are domain-specific widgets (game vitals, countdowns)
- No direct Ratatui equivalent for game-specific needs
- Custom implementation is justified

### 9. **Compass, InjuryDoll, Hand/Hands**
**Status:** ✅ Acceptable
- Game-specific visualization widgets
- Would require Canvas widget at most, but custom is fine
- These are specialized enough to warrant custom code

### 10. **Forms (Highlight, Keybind, Settings, etc.)**
**Status:** ✅ Acceptable
- Complex interactive forms
- Ratatui doesn't have form widgets
- Using external `tui-textarea` crate appropriately
- Custom implementation justified

---

## Refactoring Priority

### Phase 1: Critical (Fix Text Artifacts)
1. **TextWindow wrapping refactor**
   - Remove `wrapped_lines` cache
   - Remove `wrap_styled_spans()` function
   - Remove `rewrap_all()` function
   - Use `Paragraph::wrap(Wrap { trim: true })`
   - Remove line padding hack

2. **TextWindow scrolling refactor**
   - Replace `scroll_offset` / `scroll_position` with simple counter
   - Use `Paragraph::scroll((offset, 0))`
   - Simplify scroll_up/scroll_down to just increment/decrement

### Phase 2: Important (Code Simplification)
3. **ScrollableContainer refactor**
   - Replace with Ratatui `List` widget
   - Use `ListState` for selection/scrolling
   - Custom `ListItem` rendering for progress bars if needed

### Phase 3: Optional (Nice to Have)
4. **PopupMenu refactor**
   - Consider replacing with `List` widget
   - Keep current if List doesn't fit exactly

---

## Lines of Code Impact

### Estimated Deletions:
- `wrap_styled_spans()`: ~150 lines
- `rewrap_all()`: ~30 lines
- Scroll management: ~80 lines
- Line padding hack: ~10 lines
- ScrollableContainer scroll logic: ~50 lines

**Total: ~320 lines of complex code eliminated**

### Estimated Additions:
- Paragraph setup with wrap/scroll: ~5 lines
- List widget setup: ~10 lines

**Net: -305 lines, significantly simpler code**

---

## Testing Requirements

After refactoring:
1. ✅ Text artifacts must be eliminated
2. ✅ Scrolling must work correctly (up/down, live view)
3. ✅ Text selection must still work
4. ✅ Search highlighting must still work
5. ✅ Window resizing must work
6. ✅ All existing features must continue working

---

## Conclusion

**We've been fighting with Ratatui instead of using it properly.** The text artifacts are a direct result of our custom wrapping conflicting with Paragraph's rendering. By using Ratatui's built-in features, we'll:

1. ✅ **Eliminate the text artifact bug**
2. ✅ **Remove ~300+ lines of complex code**
3. ✅ **Use battle-tested Ratatui code**
4. ✅ **Simplify maintenance**
5. ✅ **Improve performance** (Ratatui's wrapping is likely optimized)

This should have been done from the start. My apologies for not checking Ratatui's capabilities first.
