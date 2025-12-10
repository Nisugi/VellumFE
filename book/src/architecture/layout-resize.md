# The Layout Resize Algorithm

> *"In the beginning, there was chaos—windows of fixed dimension trapped in terminals of infinite possibility. Then came the Algorithm, and the windows learned to breathe."*

## Abstract

The VellumFE Layout Resize Algorithm represents a breakthrough in terminal UI adaptation, enabling complex multi-window layouts to gracefully scale across any terminal dimension while preserving the designer's original intent. Drawing inspiration from the venerable VellumFE (Wizard Front End), this algorithm implements a **constraint-aware proportional distribution system** that handles the fundamental tension between fixed-size widgets and fluid content areas.

This document provides a complete technical specification of the algorithm, suitable for implementation, academic study, or those quiet moments when one simply wishes to contemplate the elegant mathematics of window management.

---

## Table of Contents

1. [The Problem Space](#the-problem-space)
2. [Design Philosophy](#design-philosophy)
3. [The Constraint Taxonomy](#the-constraint-taxonomy)
4. [Algorithm Overview](#algorithm-overview)
5. [Phase 1: Height Distribution](#phase-1-height-distribution)
6. [Phase 2: Width Distribution](#phase-2-width-distribution)
7. [The Cascade Mechanism](#the-cascade-mechanism)
8. [Constraint Clamping](#constraint-clamping)
9. [Remainder Distribution](#remainder-distribution)
10. [Edge Cases and Invariants](#edge-cases-and-invariants)
11. [Worked Example](#worked-example)
12. [Implementation Reference](#implementation-reference)

---

## The Problem Space

Consider a layout designed for a 122×64 terminal, containing 28 windows of varying types:

- **Text windows** that benefit from additional space (main, chat, room)
- **Progress bars** that must remain exactly 1 row tall
- **Fixed displays** like compass (4×4) and injury doll (10×7)
- **Scrollable lists** with minimum and maximum size constraints

When this layout encounters a 253×88 terminal—nearly double the width—what should happen?

### The Naive Approach (And Why It Fails)

A naive approach might scale all windows proportionally:

```
new_size = original_size × (new_terminal / original_terminal)
```

This catastrophically fails because:

1. **Progress bars become 2 rows tall** — destroying their visual design
2. **The compass becomes 8×8** — an absurd stretched ellipse
3. **Rounding errors accumulate** — leaving gaps or overlaps
4. **Constraints are violated** — windows exceed max_rows/max_cols

### The VellumFE Insight

The original Wizard Front End (VellumFE) solved this elegantly: **distribute extra space only to windows that want it, proportional to their original size, while respecting constraints.**

VellumFE implements this insight with modern enhancements for constraint-aware distribution.

---

## Design Philosophy

The resize algorithm adheres to three sacred principles:

### 1. The Principle of Proportional Intent

> *A window that occupied 20% of the available space should continue to occupy approximately 20% after resize.*

When distributing delta (additional space), windows receive shares proportional to their original dimensions. A 10-row window gets twice the delta of a 5-row window.

### 2. The Principle of Constraint Sovereignty

> *A window's constraints are inviolable. No algorithm may force a progress bar to be 2 rows tall.*

Constraints (`min_rows`, `max_rows`, `min_cols`, `max_cols`) are absolute boundaries. The algorithm must work *around* constrained windows, redistributing their would-be delta to windows that can accept it.

### 3. The Principle of Dimensional Independence

> *Height and width are separate kingdoms, each governed by their own distribution.*

The algorithm processes height changes first (column-by-column), then width changes (row-by-row). This separation prevents complex interdependencies and enables clear reasoning about each dimension.

---

## The Constraint Taxonomy

Windows are classified into categories based on their scaling behavior:

### Static Both (No Scaling)

These widgets have fixed dimensions in both directions:

| Widget Type | Reason |
|-------------|--------|
| `compass` | Fixed 4×4 graphical display |
| `injury_doll` | Fixed anatomical diagram |
| `dashboard` | Fixed stat grid |
| `indicator` | Fixed icon display |

Static widgets **receive zero delta** but **do reposition** as windows above/left of them grow.

### Static Height (Width Can Scale)

These widgets must maintain exact height but can grow horizontally:

| Widget Type | Reason |
|-------------|--------|
| `progress` | Health/mana bars are 1 row |
| `countdown` | Timer displays are 1 row |
| `hand` | Item displays are 1 row |
| `command_input` | Input line is 1 row |

### Fully Scalable

All other widgets (primarily `text` and `tabbedtext`) can scale freely in both dimensions, subject to their individual constraints.

### Constraint Capping

A window may have explicit constraints in `layout.toml`:

```toml
[window.buffs]
rows = 10
cols = 20
max_rows = 10    # Cannot exceed 10 rows
max_cols = 20    # Cannot exceed 20 columns
```

**Critical Insight**: A window *at its maximum* is effectively static for that dimension. The algorithm must detect this and exclude such windows from delta distribution—otherwise their "share" vanishes into the void.

---

## Algorithm Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    RESIZE ALGORITHM                         │
├─────────────────────────────────────────────────────────────┤
│  INPUT:                                                     │
│    • baseline_layout: Original window positions/sizes       │
│    • target_width, target_height: New terminal dimensions   │
│                                                             │
│  COMPUTE:                                                   │
│    • width_delta = target_width - baseline_width            │
│    • height_delta = target_height - baseline_height         │
│                                                             │
│  PHASE 1: HEIGHT DISTRIBUTION                               │
│    For each column 0..max_col:                              │
│      1. Find all windows occupying this column              │
│      2. Calculate total "actually scalable" height          │
│      3. Distribute height_delta proportionally              │
│      4. Apply with cascade repositioning                    │
│                                                             │
│  PHASE 2: WIDTH DISTRIBUTION                                │
│    For each row 0..max_row:                                 │
│      1. Find all windows occupying this row                 │
│      2. Calculate total "actually scalable" width           │
│      3. Distribute width_delta proportionally               │
│      4. Apply with cascade repositioning                    │
│                                                             │
│  OUTPUT:                                                    │
│    • All windows repositioned and resized                   │
│    • Layout fills target terminal exactly                   │
└─────────────────────────────────────────────────────────────┘
```

---

## Phase 1: Height Distribution

Height is distributed **column by column**, processing columns from left to right.

### Why Column-by-Column?

Windows in the same column form a vertical stack. When one grows taller, all windows below it must shift down. Processing column-by-column ensures:

1. Each vertical stack is handled independently
2. Multi-column windows are processed once (at their leftmost column)
3. Cascade effects are localized

### Step 1: Identify Windows in Column

For column `c`, find all visible windows where:

```rust
window.col <= c && window.col + window.cols > c
```

This captures windows that *span* the column, not just those starting at it.

### Step 2: Calculate Actually Scalable Height

This is where the magic happens. We sum the heights of windows that:

1. Are NOT in the `static_both` set
2. Are NOT in the `static_height` set
3. Are NOT already at their `max_rows` constraint

```rust
let mut total_scalable_height = 0;
for window in windows_at_column {
    if is_static(window) { continue; }
    if is_at_max_rows(window) { continue; }  // THE CRUCIAL CHECK
    total_scalable_height += window.rows;
}
```

**The At-Max-Rows Exclusion**: If a window has `max_rows = 10` and is currently 10 rows, it cannot grow. Including it in `total_scalable_height` would allocate delta to it that can never be used—that delta simply disappears. By excluding at-max windows, their share redistributes to windows that can actually grow.

### Step 3: Proportional Distribution

Each scalable window receives delta proportional to its size:

```rust
let proportion = window.rows as f64 / total_scalable_height as f64;
let delta = (proportion * height_delta as f64).floor() as i32;
```

Using `floor()` ensures we never over-allocate. Remainders are handled separately.

### Step 4: Apply with Constraints

When applying delta:

```rust
let new_rows = (original_rows + delta)
    .max(min_rows)    // Never go below minimum
    .min(max_rows);   // Never exceed maximum

let actually_used = new_rows - original_rows;
let remainder = delta - actually_used;
```

If constraints prevent using all assigned delta, the `remainder` is redistributed to subsequent windows in the column.

---

## Phase 2: Width Distribution

Width is distributed **row by row**, processing rows from top to bottom.

The logic mirrors height distribution exactly, but operating on the horizontal axis:

1. Find windows occupying row `r`
2. Calculate total actually scalable width (excluding `static_both` and at-max-cols windows)
3. Distribute `width_delta` proportionally
4. Apply with cascade repositioning

### The Symmetry

| Height Distribution | Width Distribution |
|---------------------|-------------------|
| Column by column | Row by row |
| `static_height` exclusion | (no equivalent—static_height windows CAN scale width) |
| `max_rows` check | `max_cols` check |
| Vertical cascade | Horizontal cascade |

---

## The Cascade Mechanism

When a window grows, all windows "downstream" must shift:

### Vertical Cascade (Height)

```
BEFORE:              AFTER (+6 to bounty):
┌─────────┐ row 0    ┌─────────┐ row 0
│ bounty  │          │         │
│ (9 rows)│          │ bounty  │
└─────────┘ row 9    │(15 rows)│
┌─────────┐ row 9    │         │
│ spells  │          └─────────┘ row 15
│(10 rows)│          ┌─────────┐ row 15  ← shifted down by 6
└─────────┘ row 19   │ spells  │
                     │(10 rows)│
                     └─────────┘ row 25
```

The cascade is implemented by tracking `current_row` as we process windows top-to-bottom:

```rust
let mut current_row = 0;
for window in windows_sorted_by_row {
    window.row = current_row;           // Position at current cascade point
    window.rows = original_rows + delta; // Apply size change
    current_row += window.rows;          // Advance cascade for next window
}
```

### Horizontal Cascade (Width)

Identical logic, but tracking `current_col` and processing left-to-right.

---

## Constraint Clamping

When applying delta to a window, constraints create boundaries:

```rust
fn apply_delta(window: &mut Window, delta: i32) -> i32 {
    let target = window.rows as i32 + delta;

    // Clamp to constraints
    let min = window.min_rows.unwrap_or(1);
    let max = window.max_rows.unwrap_or(u16::MAX);

    let clamped = target.max(min as i32).min(max as i32) as u16;

    // Return unused delta for redistribution
    let used = clamped as i32 - window.rows as i32;
    window.rows = clamped;

    delta - used  // remainder
}
```

### Remainder Redistribution

When a window can't use its full delta (due to hitting max), the remainder passes to subsequent windows:

```
Window A: assigned +10, max allows +3, remainder +7 → passes to B
Window B: assigned +8, receives +7 extra = +15 total
```

This ensures **all delta is distributed** (assuming at least one window can grow).

---

## Remainder Distribution

After proportional distribution, rounding leaves remainder:

```
height_delta = 24
total_scalable_height = 26

Window allocations (using floor):
  bounty (9):    9/26 × 24 = 8.31 → 8
  spells (10):  10/26 × 24 = 9.23 → 9
  targets (7):   7/26 × 24 = 6.46 → 6
                              Total: 23

Remainder: 24 - 23 = 1
```

The remainder is distributed round-robin to scalable windows:

```rust
while remainder > 0 {
    for window in scalable_windows {
        if remainder == 0 { break; }
        window.delta += 1;
        remainder -= 1;
    }
}
```

This ensures exact distribution with no pixels lost.

---

## Edge Cases and Invariants

### Invariant 1: Total Delta Conservation

The sum of all applied deltas must equal the original delta (unless ALL windows are at max).

```
∑(applied_delta) = height_delta  (or width_delta)
```

### Invariant 2: Constraint Satisfaction

No window may violate its min/max constraints after resize.

```
∀ window: min_rows ≤ window.rows ≤ max_rows
∀ window: min_cols ≤ window.cols ≤ max_cols
```

### Edge Case: All Windows At Max

If every window in a column is at `max_rows`, then `total_scalable_height = 0`. The algorithm skips this column—there's nowhere for delta to go.

**Visual result**: A gap appears at the bottom of the column. This is correct behavior; the constraints make growth impossible.

### Edge Case: Negative Delta (Shrinking)

When `height_delta < 0`, the same algorithm applies in reverse:

- Proportional *reduction* instead of growth
- `min_rows` becomes the binding constraint
- Cascade moves windows *up* instead of down

### Edge Case: Multi-Span Windows

A window spanning columns 0-10 is processed when column 0 is evaluated. It's marked as "already applied" and skipped in columns 1-10. This prevents double-application.

---

## Worked Example

### Initial State

Terminal: 122×64
Layout baseline: 122×64

| Window | Row | Rows | Col | Cols | max_rows | max_cols |
|--------|-----|------|-----|------|----------|----------|
| bounty | 0 | 9 | 0 | 20 | — | 20 |
| active_spells | 9 | 10 | 0 | 20 | — | 20 |
| buffs | 19 | 10 | 0 | 20 | 10 | 20 |
| targets | 29 | 7 | 0 | 20 | — | 20 |
| room | 0 | 14 | 20 | 102 | — | — |
| main | 14 | 50 | 20 | 102 | — | — |

### Resize Target

New terminal: 253×88

```
width_delta  = 253 - 122 = +131
height_delta = 88 - 64   = +24
```

### Phase 1: Height Distribution (Column 0)

**Scalable windows** (excluding buffs at max_rows=10):
- bounty: 9 rows
- active_spells: 10 rows
- targets: 7 rows
- **Total: 26 rows**

**Proportional allocation** of +24:
- bounty: (9/26) × 24 = 8.31 → **+8**
- active_spells: (10/26) × 24 = 9.23 → **+9**
- targets: (7/26) × 24 = 6.46 → **+6**
- **Subtotal: 23** (remainder: 1, distributed to bounty → **+9**)

**Results**:
| Window | Old Rows | Delta | New Rows | New Row Position |
|--------|----------|-------|----------|------------------|
| bounty | 9 | +9 | 18 | 0 |
| active_spells | 10 | +9 | 19 | 18 |
| buffs | 10 | 0 | 10 | 37 |
| targets | 7 | +6 | 13 | 47 |

### Phase 2: Width Distribution (Row 0)

**Scalable windows** (excluding bounty at max_cols=20):
- room: 102 cols
- **Total: 102 cols**

**All +131 goes to room**:
- room: 102 + 131 = **233 cols**

### Final State

| Window | Position | Size |
|--------|----------|------|
| bounty | (0, 0) | 20×18 |
| active_spells | (0, 18) | 20×19 |
| buffs | (0, 37) | 20×10 |
| targets | (0, 47) | 20×13 |
| room | (20, 0) | 233×... |
| main | (20, ...) | 233×... |

The layout now fills 253×88 perfectly, with text windows absorbing all extra space while constrained windows maintain their designed dimensions.

---

## Implementation Reference

The algorithm is implemented in:

```
src/core/app_core/layout.rs
```

Key functions:

| Function | Purpose |
|----------|---------|
| `resize_windows_to_terminal()` | Entry point, calculates deltas |
| `apply_height_resize()` | Phase 1: column-by-column height distribution |
| `apply_width_resize()` | Phase 2: row-by-row width distribution |
| `widget_min_size()` | Returns intrinsic minimum size for widget types |
| `sync_layout_to_ui_state()` | Applies final positions to UI |

### Debug Logging

Enable detailed resize logging:

```bash
# Unix/Linux/macOS
RUST_LOG=vellum_fe::core::app_core::layout=debug cargo run

# Windows PowerShell
$env:RUST_LOG="vellum_fe::core::app_core::layout=debug"; cargo run
```

This produces output showing:
- Delta calculations
- Per-window proportional allocations
- Constraint clamping events ("at max_rows, delta=0")
- Final positions

---

## Conclusion

The VellumFE Layout Resize Algorithm transforms the chaos of arbitrary terminal dimensions into ordered, beautiful interfaces. By respecting constraints, distributing proportionally, and cascading intelligently, it achieves what might seem impossible: layouts that feel *designed* for every terminal size.

The next time you resize your terminal and watch your carefully crafted HUD adapt seamlessly, spare a thought for the mathematics flowing beneath—the proportions calculated, the constraints honored, the remainders distributed.

For in the end, a good resize algorithm is invisible. And invisibility, in software, is the highest form of success.

---

*"The window that adapts survives. The window that fights the terminal perishes."*
— Ancient Proverb (c. 2024)
