# Hands

> Know what's in each hand and what spell you have prepared, at a glance — instead of burning
> a round on `inventory` to find out you're holding a shield you meant to drop.

## What it's for

You swap weapons, you get disarmed, you prepare a spell and then take a hit and lose track of
whether it's still there. The game will tell you, but only if you ask, and asking costs you
the moment you needed the answer in.

The hand windows keep all three slots on screen: left, right, and the spell you have prepared.
They're one line each, so they cost almost nothing, and the item names stay clickable. In the
desktop GUI they can go further — the icon itself can change based on what you're holding, so
"empty hand" and "axe in hand" look different from across the screen.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar and tick **Left Hand**, **Right Hand**, and **Spell**
   in the catalog. Set each row's **zone** — stacking all three in the **Right Bar** keeps
   them together. (Typed equivalent: `.addwindow left hand 92 22 28 1`.)
2. Right-click a hand window and choose **Hand icons…** in the widget section. That opens the
   editor for status-driven icons: pick a condition, pick the art it should show.
3. For a frame around the window, right-click ▸ **Appearance** ▸ **Frame**.

<figure class="shot" data-shot="widgets/hands-icons-editor">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The <b>Hand icons…</b> editor with a state condition on the left and the <b>hands</b> image pool on the right.</figcaption>
</figure>

→ **Expected result:** three one-line windows showing what you're holding. Picking up a weapon
updates the line immediately, and clicking the item name opens its verb menu.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow left hand 92 22 28 1` for the left hand, then repeat with `right` and `spell`
for the other two. Type `.addwindow` with no arguments for a picker instead.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow left` alone
> prints usage and adds nothing.

`.editwindow left` opens the form for the icon prefix and colors; `Tab` and `Shift+Tab` move,
`Ctrl+S` saves, `Esc` cancels. The terminal shows a text prefix — `L:`, `R:`, `S:` — followed
by the item name.

> ⚠️ **The terminal draws text prefixes, never icon art — by design, not a gap.** Hand icons
> are images from a pool, so the **Hand icons…** editor is GUI-only. A layout that defines
> icon states still works here: the terminal uses each state's **text** prefix and ignores its
> image, and saving from the terminal leaves those states untouched.

→ **Expected result:** three one-line windows reading like `R: a vultite waraxe`, with an empty
hand showing just the prefix.
{{#endtab}}
{{#tab name="Mobile"}}

The phone uses fixed chrome, so you don't place these windows — hands are already in two places.
The top status strip shows compact **L** and **R** slots beside your vitals, and the **status
drawer** (swipe in from the right) carries a **Hands** section with `L:` and `R:` rows.

The phone shows the two physical hands, not the prepared spell. Icon states are desktop
authoring; set them up in the GUI.

→ **Expected result:** the L and R slots in the status strip track what you're holding, showing
a dash when a hand is empty.
{{#endtab}}
{{#endtabs}}

## Common setups

### Three lines stacked under your vitals

Add all three hand windows and send them to the same zone, one under another, in the order
left, right, spell. Right-click each ▸ **Appearance** ▸ **Frame** and turn the border off so
they read as one block instead of three boxes.

**You'll see:** a three-line readout under your bars — both hands and your prepared spell,
updating as you swap gear and cast.

### An icon that changes when your hand is empty

Right-click your right-hand window ▸ **Hand icons…** and add two states, in this order:

1. **Hand empty** (right) → pick an open-hand image.
2. **Hand holds** → weapon (right) → pick a weapon image.

Save, then drop what you're holding.

**You'll see:** the icon flip to the open hand the moment the weapon leaves, and flip back when
you pick it up — visible from across the screen without reading a word.

## Tips & gotchas

> ⚠️ **The first matching state wins.** States are checked top to bottom, and a broad condition
> placed above a narrow one swallows it. Put **Hand holds → a specific item** above **Hand
> holds → weapon**, or the specific rule will never fire. If nothing matches, the window falls
> back to its static icon and colors.

> ⚠️ **Which hand a window shows comes from its name, and the fallback is the spell hand.** A
> name containing "left" is the left hand, one containing "right" is the right hand, and
> **anything else — including a typo — becomes the spell hand**. If a window named `rigth` is
> showing your prepared spell, that's why.

**Drag a hand window taller to get bigger art.** In the GUI the icon fills the window's height,
so a one-line hand gets a small icon and a window dragged to two or four lines gets large art.
The configured icon size acts as the floor, not the size.

**Hand icon states use the same condition language as hotbar buttons and indicators.** Learn it
once and it transfers — **Hand empty**, **Hand holds**, **Spell prepared**, **Roundtime
active**, **Vital**, **Injury**, and the rest, grouped with **all of** / **any of**. If you've
built a [combat hotbar](../how-to/combat-hotbar.md), you already know this editor.

**Empty reads differently in the spell slot.** The two physical hands show `Empty` when you're
holding nothing; the spell window shows `None` when you have nothing prepared.

## See also

- [Wire a hotbar for combat](../how-to/combat-hotbar.md) — the same condition vocabulary, on
  buttons
- [Indicators](./indicators.md) — status icons driven by those same conditions
- [Inventory](./inventory.md) — everything you're carrying, not only what's in hand
- [Skins (GUI Graphics)](../customization/skins.md) — where hand icon art comes from
- [Build a hunting layout](../how-to/hunting-layout.md) — where the hand windows sit in a combat
  screen

<details>
<summary>Config reference (TOML)</summary>

`widget_type = "hand"`. Set these through **Hand icons…** and the right-click menu (GUI) or
`.editwindow` (TUI).

| Field | Type | Default | What it does |
|---|---|---|---|
| `icon` | string | from template | Static prefix, for example `"L:"`, `"R:"`, `"S:"` |
| `icon_color` | string | window color | Icon color |
| `hand_text_color` | string | window color | Item text color. Also accepts the old name `text_color`. |
| `states` | array | empty | Condition-driven icon states, first match wins |

**Which hand:** taken from the window `name` — `left` → left hand, `right` → right hand,
anything else → spell hand.

Each entry in `states` takes `when` (a condition), plus any of `icon` (GUI image), `text` (TUI
prefix), and `icon_color`. A matched state replaces the static icon while its condition holds;
no match falls through to the static settings above.

```toml
[[windows]]
name = "right"
widget_type = "hand"
icon = "R:"
row = 22
col = 92
rows = 1
cols = 28
show_border = false

  [[windows.states]]
  text = "(  )"
  [windows.states.when]
  type = "hand_empty"
  hand = "right"

  [[windows.states]]
  text = "R:"
  icon_color = "#c0392b"
  [windows.states.when]
  type = "hand_holds"
  hand = "right"
  item_type = "weapon"
```

</details>
