# Containers Window

> Your whole pack as one collapsible tree — every bag, every nested pouch, with weights and
> capacities lined up in a column so you can see which container is about to overflow.

## What it's for

You know roughly what you're carrying. What you don't know is which of your five bags the spare
gems went into, or which one is 2 lbs from full. Asking the game means a command per container and
a screenful of scrollback per answer.

A containers window answers all of it at once. It takes one structured snapshot of everything you
own, draws it as a tree you can open and close, and prints each container's item count, carried
weight, and capacity in aligned columns. A bag past 90% of its capacity turns its weight red — you
see the problem before you try to stow anything.

> ⚠️ **This is not the [Container Windows](./container-windows.md) widget (singular `container`).**
> That one opens a **separate window per bag**, session-only, and you turn each bag on by ticking
> its row. This page is `containers` (plural): **one** window holding the whole inventory as a
> tree. They have similar names and do different jobs. If you want a dedicated window pinned to
> your gem pouch, you want [the other page](./container-windows.md).

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar. The catalog opens **collapsed** — expand the **Character**
   group, then tick **Containers**. It arrives 20 rows by 40 columns.
   (Typed equivalent: `.addwindow containers containers 100 0 40 20`.)
2. The window opens showing **⟳ Refresh**, the words **no snapshot yet**, and the line
   **Refresh to load the structured inventory.** That is the correct empty state — no snapshot has
   been taken.
3. Click **⟳ Refresh** (or type `.invsync`). The tree fills in.
4. There is no widget section in this window's right-click menu — `containers` has no settings of
   its own. Right-click gives you **Appearance** and the geometry entries only.

<figure class="shot" data-shot="widgets/containers-window-gui-empty-state">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A freshly added <b>Containers</b> window before any sync: the <b>⟳ Refresh</b> button, <b>no snapshot yet</b>, and the prompt to refresh.</figcaption>
</figure>

<figure class="shot" data-shot="widgets/containers-window-gui-tree-filled">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The filled tree on the <b>Containers</b> tab: a <b>Worn containers</b> heading, one bag expanded, and the <code>[N]&nbsp;&nbsp;W.W / C</code> stat columns aligned down the right edge.</figcaption>
</figure>

→ **Expected result:** four tabs — **Containers**, **Worn**, **Room**, **Item** — with the first
one showing your hands, worn containers, items at your feet, and reserved items as an expandable
tree. Each container row reads `[N]  W.W / C`.

{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow containers containers 100 0 40 20` — name, type, then column, row, width, height.
Type `.addwindow` with no arguments for a picker instead, then choose **Containers**.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow containers` alone
> prints usage and adds nothing.

Before any sync the window holds one grey line: `.invsync to load`. Type `.invsync` and it fills.

**The terminal tree is always fully expanded and has no stat columns.** Where the GUI prints
`[N]  W.W / C` in aligned cells, the TUI writes one flat sentence per container row:
`<name> [closed] (3) 12.4/50 lbs` — state tag, nested count in parentheses, then carried weight and
capacity. A container with no known capacity ends in ` lbs` with no slash. Section headings arrive
as `═ WORN CONTAINERS ═` bands, and the window title carries the item total: `Containers (147)`, or
`Containers (147, incomplete)` when the snapshot did not finish.

**Rows are not clickable here.** The terminal build renders every line without a link, so there is
no verb menu, no inspector, and no Focus mode — those are GUI-only. Nesting is shown by two spaces
of indent per level.

→ **Expected result:** a bordered window titled `Containers (N)` listing every container in blue,
closed and locked ones in orange, with their contents indented beneath them.

{{#endtab}}
{{#tab name="Mobile"}}

There is no containers surface on the phone — no drawer section, no wheel slice, no tree. The
managed-inventory snapshot is a desktop window only, and `.invsync` has nothing to draw into there.

What the phone carries instead is the prose feed: the **Inventory** stream chip, in the stream
filter chips row, prints what you're carrying as the game sends it. The touch wheel's **Inventory**
slice is the fast route — it switches you straight to that chip. For "which bag is this in?", type
the question to the game in the command input and read the reply in the story pane.

→ **Expected result:** the touch wheel's **Inventory** slice switches the view to the **Inventory**
chip, showing your carried items as text. There are no capacity columns and no collapsible bags.

{{#endtab}}
{{#endtabs}}

## Common setups

### A "what's in which bag" column you refresh once per stop

The tree is a snapshot, not a live feed, so it pays to treat it like a map you re-draw when you
arrive somewhere rather than something you watch.

1. Add **Containers** from the catalog's **Character** group and send it to a tall zone — the
   **Right Bar** suits a 20-row window.
2. Sync once when you sit down to sort loot: click **⟳ Refresh**, or type `.invsync`.
3. Collapse the bags you don't care about. Only the bag you're working out of stays open.

**You'll see:** one column where every bag you own shows its item count and its weight against its
capacity, and the bag you're filling turns its weight amber as it crosses 70% and red past 90%.

### Digging through one bag without the rest in the way

1. In the GUI, right-click the bag you're working out of and choose **Focus**.
2. The window swaps to that bag alone — its whole subtree flattened and sorted by name, with a
   header reading `<bag name> — 34 items · 41.2 lbs`. Anything nested deeper than the bag itself
   gets `· in <holder>` appended so you know which inner pouch it sits in.
3. Click **◀ All containers** to come back.

**You'll see:** one alphabetical list of everything inside that bag and nothing else on screen,
with each entry's weight in parentheses.

## Tips & gotchas

> ⚠️ **Without the extended feed the window stays empty forever, and it is not broken.**
> `.invsync` asks the game a question only the extended feed answers, and the game only opens that
> feed for a client that sent the **WRAYTH** banner. Direct mode sends it; Lich can too. Connect
> some other way and `.invsync` prints its request line, the server never replies, and you are left
> looking at **no snapshot yet**. This is the single most common reason this window looks dead.
> The window says so itself, under the empty state.

> ⚠️ **There is no auto-refresh, and that is deliberate.** The window is read-only over the last
> snapshot. Pick something up, drop something, or stow a gem, and the tree does not notice — it
> keeps showing the world as it was at the last sync. Refresh when you want the truth.

> ⚠️ **Two syncs cannot overlap.** Ask for a second refresh while one is running and you get
> `[invsync] refresh already in progress.` and nothing else happens. Wait for the first to finish.

> ⚠️ **Room items disappear when you walk away, on purpose.** The snapshot records which room it
> was taken in. Move to a different room and the **Room containers** and **On the ground** sections
> are suppressed, because they describe scenery you have left. The GUI says
> **Room items are from another room - Refresh here to load them.**; the TUI says
> `room items omitted (synced elsewhere - .invsync)`. Sync again where you are standing.

> ⚠️ **The terminal build shows the tree and nothing more.** Clicking, the verb menu, **Focus**,
> the **Item** inspector tab, and the **⟳ Refresh** button are all GUI-only. In the TUI you drive
> everything from the command line with `.invsync`, `.viewitem`, and `.drag`.

**A snapshot can be incomplete.** Both frontends say so — the GUI header appends `(incomplete)`
after the item count, the TUI puts `, incomplete` in the title. It means the continuation-following
did not reach the end. Refresh again.

**Locked containers report `locked` where the count would be.** The game does not tell you what is
inside a container you cannot open, so the window does not guess.

**Item actions go through `.drag`, which waits for proof.** Right-click ▸ **To right hand**,
**To left hand**, **Drop**, and **Wear** each fire a `.drag` that is confirmed against your hand
updates rather than against prose. A move that the game silently refused does not report success.

**Left-clicking an item jumps to the Item tab.** In the GUI, clicking any non-container row runs
`.viewitem` for it and switches the window to the **Item** tab, where the look, inspect, analyze,
and read results stack under their own headings. Right-click ▸ **Inspect** does the same.

**Collapsed is the default.** Every container row starts closed the first time you see it, so a
large inventory opens as a short list of bags rather than a wall of gems.

**Weight columns use one decimal on every row** — item rows included — so the decimal points form a
single rail down the window and you can compare two bags by eye.

**`.find <name fragment>` searches the snapshot** and prints each match's container path, with
closed containers flagged. It reads the same snapshot this window draws, so run `.invsync` first.

## See also

- [Container Windows (per-bag)](./container-windows.md) — the singular `container` widget: one window per bag
- [Inventory Tools](../features/inventory-tools.md) — `.invsync`, `.find`, `.viewitem`, and `.drag` in full
- [Inventory](./inventory.md) — the plain carried-items feed
- [Reserve](./reserve.md) — what the game is holding aside for you
- [Items](./items.md) — what's on the ground rather than what's yours

<details>
<summary>Config reference (TOML)</summary>

Written by the catalog and `.addwindow`. Hand-editing is for troubleshooting, not the normal path.

```toml
[[windows]]
name = "containers"
widget_type = "containers"
row = 0
col = 100
rows = 20
cols = 40
show_border = true
title = "Containers"
```

### Widget fields

**There are none.** The `containers` widget data shape is an empty struct — it carries no
widget-specific keys at all, which is why the GUI right-click menu has no widget section for this
type and `.editwindow` offers nothing beyond the standard window form. Everything the window shows
comes from the snapshot; everything about how it looks comes from the shared window keys.

The type string is exactly `containers`; an unrecognized type does not error, it quietly creates a
**text** window instead. Note that `container` (singular) is a different, valid type — a per-bag
window, not this tree.

Standard window keys — `row`, `col`, `rows`, `cols`, `show_border`, `border_style`, `title`,
`locked` — apply as they do to any window.

### Catalog preset

| Catalog row | Category | Title | Size | Game |
|---|---|---|---|---|
| Containers | Character | Containers | 20 rows x 40 cols (floor: 5 rows x 20 cols) | GemStone IV only |

The game gate hides the row from DragonRealms characters in both the GUI catalog and the
bare-`.addwindow` picker. **An unset game counts as GemStone IV**, so the row appears by default.

### Where the settings live

| Setting | Desktop GUI | Terminal (TUI) |
|---|---|---|
| Refresh the snapshot | **⟳ Refresh** button in the window | `.invsync` |
| Title, border, geometry | Right-click ▸ **Appearance** / drag | `.rename`, `.border`, `.movewindow` |
| Widget-specific options | none exist | none exist |

### Related commands

| Command | What it does |
|---|---|
| `.invsync` | Requests a fresh snapshot. Refuses to overlap an in-flight refresh. Needs the extended feed. |
| `.viewitem <exist-id>` | Loads an item into the **Item** tab and routes to the `inspect` stream. |
| `.find <name fragment>` | Searches the current snapshot and prints each match's container path. |
| `.drag <exist> left\|right\|drop\|wear\|feet` | Verified item move, confirmed against hand updates. |
| `.drag <exist> in\|on\|behind\|underneath <dest>` | Verified move into another container. |

</details>
