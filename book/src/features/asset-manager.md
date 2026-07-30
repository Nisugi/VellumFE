# Asset Manager (`.jinx`)

VellumFE can download and keep current the files that shape your
game and your interface — **game data** (item and spell databases,
the map), **skins**, **icon sets**, and shareable **layouts / UI
packs** — without a Lich install. Direct connections and phones get
the same fresh files everyone else does.

It speaks the same federated-repository protocol as Lich's
[`jinx`](https://gswiki.play.net/Lich_(software)) package manager, so
it reads the **existing community repositories** for game data *and*
VellumFE's own repositories for skins, icons, and layouts. If you *do*
run through Lich, `;jinx` still works exactly as before — this is the
native, `.`-prefixed equivalent, and you can use whichever you prefer.

> **Nothing downloads on its own.** Every install and update is
> something you ask for (or opt into with `.jinx auto-update`). VellumFE
> never reaches out to the network unless you tell it to.

## Quick start

```
.jinx list                       see everything available
.jinx search parchment           find assets by name
.jinx info parchment             read details before installing
.jinx install parchment          install it
.jinx update parchment           pull the latest version
.jinx auto-update --dry-run       see what has updates (without applying)
.jinx auto-update                update everything you've installed
```

## What you can install

| Kind | Examples | Where it lands |
|------|----------|----------------|
| **Game data** | `gameobj-data.xml`, `effect-list.xml`, `spell-list.xml`, `mapdb.json` | shared data store; loaded live |
| **Skins** | full GUI graphics themes (frames, colors, icons) | `global/skins/<name>/` |
| **Icon sets** | shared icon sheets for hotbars / status / hands | `global/images/icons/` |
| **Layouts / UI packs** | complete window arrangements shared as `.vellumpack` | your layouts folder |

Game-data files are the same ones Lich maintains, so if you have a Lich
install keeping them fresh, VellumFE already prefers that copy — see
[the data pack](./inventory-tools.md#the-data-pack-data). `.jinx` is how
you keep them current *without* Lich.

> VellumFE deliberately installs only data and interface assets — it
> **never** downloads or runs scripts. Executable code stays Lich's
> domain.

## Commands

### Finding and inspecting

```
.jinx list [--repo=<name>]        list all assets (optionally one repo)
.jinx search <pattern>            search names across every repo
.jinx info <name>                 type, age, author, description
```

`info` shows the author and description for VellumFE assets (skins,
layouts) so you know what a thing is before you install it.

### Installing and updating

```
.jinx install <name> [--repo=<name>] [--force]
.jinx update  <name> [--force]
```

- The **kind** (data / skin / icon / layout) is detected automatically
  from the repository — you just name the asset.
- If the same name exists in two repositories, add `--repo=<name>` to
  pick one. `.jinx` will tell you when it needs that.
- `--force` overwrites even when the local copy looks changed.

When you install a **skin**, VellumFE asks whether to switch to it — it
never restyles your interface without your say-so.

### Keeping everything current

```
.jinx auto-update [--dry-run]
```

Checks every asset you've installed against its repository and updates
the ones that changed. `--dry-run` reports what *would* update and
changes nothing — a good first step.

### Managing repositories

VellumFE ships with the standard repositories already configured. To add
a community repo (for example, a friend's skin collection):

```
.jinx repo list                          show configured repositories
.jinx repo add <name> <https-url>        add one (HTTPS only)
.jinx repo change <name> <https-url>     point a repo at a new URL
.jinx repo rm <name>                     remove one
```

Only `https://` URLs are accepted.

## How updates know what changed

Each repository publishes a small manifest listing every asset and a
checksum of its current contents. VellumFE compares that checksum to
what you have installed:

- **clean** — your copy matches the repository.
- **modified** — your copy differs (you edited it, or it's out of date).
- **update available** — the repository has a newer version.

For multi-file assets like skins, the whole bundle is checksummed as a
unit, so *any* change to *any* file shows up as a single "update
available." Before an update overwrites anything, VellumFE backs up what
it replaces, so trying a new version is always safe to undo.

## Sharing your own skins, icons, and layouts

VellumFE's community assets live in a single public repository, and
contributions are welcome. The short version:

1. Build your asset. A layout or UI pack is just a `.vellumpack` from
   [`.uiexport`](./ui-packs.md). A skin is its skin folder.
2. Add a small `meta.toml` next to it — title, author, description,
   version, and a `preview.png`.
3. Open a pull request to the community assets repository.

A validation check confirms the structure (valid `meta.toml`, a preview
image, sensible size). Once merged, an automated job rebuilds the
manifest, and your asset is live in everyone's `.jinx list` — no server
to run, no manual step.

See the [contributor guide](#contributing-details) below for the exact
`meta.toml` fields and folder layout.

## Contributing details

A contributor writes exactly one file by hand — everything else
(checksums, timestamps, the manifest entry) is generated automatically.

**`meta.toml`**, placed beside your asset:

```toml
title       = "Parchment"                        # shown in the gallery
author      = "YourName"
description = "Warm aged-paper theme with rune icons."
version     = "1.2.0"                             # bump when you change it
tags        = ["warm", "fantasy", "high-contrast"]
preview     = "preview.png"                       # ships inside the asset
```

Folder layout in the community repository:

```
skins/
  parchment/
    meta.toml
    preview.png
    skin.toml
    ...skin files...
icons/
  <your-icon-set>/meta.toml, ...
layouts/
  <your-layout>.vellumpack + meta.toml
```

**Requirements the validator checks:**

- a valid `meta.toml` with `title`, `author`, `description`, `version`
- a real `preview.png` (for skins and layouts), reasonably sized
- the asset stays within its folder and under the size cap
- for skins, a `skin.toml` that parses

When your PR merges, the manifest rebuilds automatically and the asset
appears in `.jinx list` for everyone.

## Troubleshooting

- **"more than one repo has asset(...)"** — add `--repo=<name>`.
- **"... already exists" / looks modified** — you (or an update) changed
  the local copy; re-run with `--force` to overwrite. Your old copy is
  backed up first for multi-file assets.
- **A manifest fails to load** — the repository may be temporarily down;
  `.jinx list` skips it and reports the error. Try again later, or check
  `.jinx repo list` for the URL.
- **Nothing seems to update** — run `.jinx auto-update --dry-run` to see
  the comparison; "clean" everywhere means you're current.
