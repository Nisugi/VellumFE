# Vellum Despana

> Use Vellum's session engine through a dense, desktop-oriented browser
> workspace.

## What it is

**Vellum Despana** is an optional browser frontend built into VellumFE. It
is intended for desktop play: the Story feed stays central while movable panels
keep room, character, combat, spell, inventory, and map information visible.

Despana is a presentation surface, not a separate game client. Vellum owns the
connection, login, XML parsing, authoritative state, command dispatch, maps,
reconnection, and authenticated web server. Despana renders that state and
sends interactions back through Vellum's public web protocol.

Despana has **no dependency on a custom Lich script or companion service**. It
works with a Direct connection. A Lich connection adds whatever scripts and
streams that particular Lich session provides, but none are required for the
frontend to render normal game state.

## Start it from the Launcher

1. Run `vellum-fe` with no arguments to open the native **VellumFE Launcher**.
2. Create a connection, or edit an existing one.
3. Choose **Direct** to log in through play.net, or **Lich** to attach to a
   detachable-client Lich port.
4. Open **Advanced** and select **Vellum Despana** under **Frontend**.
5. Save and launch the connection.

Vellum starts the session and opens an authenticated Despana tab after its web
server has bound. Use the URL Vellum opens rather than constructing one by
hand; it contains the actual port and pairing token for that session.

The canonical route is `/despana`.

## Direct and Lich connections

Both connection modes use the same Despana interface and the same Vellum state
model:

| Mode | Vellum does | What to expect |
|---|---|---|
| **Direct** | Authenticates with play.net and connects without Lich | Normal game state and commands work; Lich scripts are unavailable |
| **Lich** | Attaches to the configured detachable-client host and port | Normal game state plus output and behavior supplied by that Lich session |

Selecting Despana does not change the connection mode or create a second
login. The saved Launcher connection remains authoritative.

## The workspace

The workspace is made from modules arranged in top, bottom, left, right, and
center zones. Use a module's menu to move or hide it, change a zone's split
direction, or restore hidden modules. Resize handles adjust neighboring zones
and modules.

Layout changes are saved automatically per character. The browser keeps an
immediate local copy, while Vellum stores the authenticated cross-port copy in
that character's profile. Reloading or using a different local Vellum port
therefore preserves the most recent layout without putting workspace data in
request cookies. **Workspace → Restore default** returns to the shipped layout
without changing game or character settings.

### Story, Room, and commands

- **Story** is the chronological game feed. It includes normal game output,
  room transitions, combat, conversations, and script output. It follows new
  text while you are at the bottom. Scrolling away or choosing **Pause** stops
  that follow intentionally; **Bottom** resumes it.
- **Room** is a current-state view. It replaces its contents as room state
  changes instead of accumulating history.
- The command input is attached to the bottom of Story. Press `Enter` or choose
  **Send** to dispatch through Vellum. Links, exits, and item action menus use
  the same command path.

## Maps

The Map module offers two views of Vellum's map data:

- **Local** draws the nearby room graph and follows the current room.
- **Classic** displays an available annotated map image and marks the current
  room on it.

Choose **Local** or **Classic** in the Map header. The map selector can display
a different available map without moving the character, and **Center** returns
the viewport to the current room. Drag to pan and use the mouse wheel to zoom.
Map availability depends on the map data installed for the current game and
location.

## Closing a session

Closing the browser tab does not stop Vellum's session process. Use `.quit` to
disconnect and return Despana to its Launcher handoff, or `.exit` when you
intend to close the session process completely.

## Troubleshooting

**The Launcher says it is waiting for Lich.** Confirm the saved connection's
host and detachable-client port, and make sure Lich is listening there. If the
profile has a custom launch command, check that it starts the same character on
the same port.

**The browser is denied or never connects.** Launch Despana again from the
native Launcher so Vellum can open a fresh paired URL. Do not copy a token from
another session or port.

**Despana says there is no active game session.** Start or attach the character
from the native VellumFE Launcher. The link to `/play` opens Vellum's browser
login in a separate page for manual recovery; Despana never embeds or owns a
second login session.

**Story stopped following new text.** Choose **Bottom**. Scrolling away from the
latest line or selecting **Pause** intentionally suspends follow mode.

**A map view is empty.** Try the other map mode and confirm map data is
installed. Classic images are not available for every location; Local view
also needs rooms in Vellum's map database.

**Lich script output is missing in Direct mode.** This is expected. Direct mode
does not run or attach to Lich. Edit the saved connection and choose **Lich** if
you want to use that installation's scripts.

## See also

- [The Launcher](../getting-started/launcher.md)
- [First Launch](../getting-started/first-launch.md)
- [Browser Client](./web.md) — Vellum's mobile and sidecar browser surface
- [Map](../widgets/map.md)
