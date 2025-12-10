# Glossary

Terms and definitions used throughout VellumFE documentation.

## A

### Aho-Corasick
A string-matching algorithm that can match multiple patterns simultaneously. Used by VellumFE for efficient highlight pattern matching when `fast_parse = true`.

### ASCII
American Standard Code for Information Interchange. A character encoding standard. VellumFE can use ASCII-only mode for maximum terminal compatibility.

### Auto-reconnect
Feature that automatically reconnects to the game server if the connection is lost.

### Auto-scroll
Widget behavior that automatically scrolls to show the newest content as it arrives.

## B

### Browser
In VellumFE, a popup window that can be opened for detailed interaction with game data. Examples: highlight editor, spell list browser.

### Buffer
Memory area for temporarily storing data. VellumFE uses buffers for incoming game data, scrollback history, and command history.

## C

### Casttime
Delay after casting a spell before you can act again. Displayed by countdown widgets.

### Character Code
Unique identifier for a character in the game's authentication system.

### Command Input
Widget type for entering commands to send to the game.

### Command List (cmdlist)
Configuration that maps context menu options to game objects based on noun matching.

### Compass
Widget displaying available movement directions in the current room.

### Config
Short for configuration. TOML files controlling VellumFE behavior.

### Core Layer
The middle layer of VellumFE architecture, containing business logic and widget management.

## D

### Dashboard
Widget type that combines multiple status elements into a single display.

### Data Layer
The foundation layer of VellumFE architecture, containing game state and parsing.

### Data Source
Configuration path identifying where a widget gets its data (e.g., `vitals.health`).

### Direct Mode
Connection method that authenticates directly with eAccess servers, bypassing Lich.

## E

### eAccess
Simutronics authentication server for GemStone IV and DragonRealms.

### Effect
Active spell, buff, or debuff affecting your character. Tracked by active_effects widget.

## F

### Fast Parse
Optimization mode using Aho-Corasick algorithm for pattern matching instead of regex.

### Focus
The currently selected/active widget. Input goes to the focused widget.

### Frontend Layer
The presentation layer of VellumFE architecture, handling rendering and input.

## G

### Game State
Central data structure containing all parsed game information.

### Generation
Version number that increments when data changes. Used for efficient change detection.

### GemStone IV
Text-based MMORPG that VellumFE is designed for.

### Glob
Pattern matching syntax using wildcards (* and ?).

### Glyph
Visual representation of a character in a font.

## H

### Hash Key
In eAccess authentication, a 32-byte key used to obfuscate the password.

### Hex Color
Color specified using hexadecimal notation (e.g., `#FF0000` for red).

### Highlight
Pattern-based text styling that colors matching text in game output.

### Hook
In some contexts, a callback function. Not currently used in VellumFE.

## I

### Indicator
Widget showing boolean status (on/off states like hidden, stunned, prone).

### Injury Doll
Widget displaying injuries to different body parts.

### Input Widget
Widget that accepts text input from the user.

## K

### Keybind
Association between a key combination and an action or macro.

### Keepalive
Network message sent periodically to prevent connection timeout.

## L

### Layout
Configuration defining widget positions, sizes, and arrangement.

### Lazy Render
Optimization that only redraws changed portions of the screen.

### Lich
Ruby-based proxy and scripting platform for GemStone IV.

### Lich Mode
Connection method that connects through Lich proxy.

## M

### Macro
Command or sequence of commands triggered by a keybind.

### Mana
Magical energy resource. Tracked by vitals.mana.

### Monospace
Font where every character has the same width. Required for terminal UI.

### MUD
Multi-User Dungeon. Text-based multiplayer games. GemStone IV is technically a MUD.

## N

### Nerd Font
Font family including many additional symbols and icons.

### Node
Individual XML element in the game protocol.

## O

### OWASP
Open Web Application Security Project. Security standards referenced in development.

### Overlay
Widget that draws on top of other widgets.

## P

### Palette
Set of colors available for use in themes.

### ParsedElement
Internal representation of processed game data.

### Parser
Component that interprets XML game protocol into structured data.

### Pattern
Regular expression or literal string for matching text.

### Percentage Positioning
Widget placement using percentages instead of fixed coordinates.

### PEM
Privacy-Enhanced Mail. Certificate file format used for TLS.

### Popup
See Browser.

### Preset
Named color with associated hex value (e.g., "health" → "#00ff00").

### Progress Bar
Widget showing a value as a filled bar (health, mana, etc.).

### Protocol
Rules for communication between client and server.

### Proxy
Intermediary that relays connections (Lich acts as a proxy).

## R

### Reconnect
Re-establishing connection after disconnection.

### Regex
Regular expression. Pattern matching syntax for text.

### Render
Drawing the UI to the screen.

### Render Rate
How many times per second the UI updates (FPS).

### Room
In-game location. Room data includes name, description, exits.

### Roundtime
Delay after an action before you can act again.

### RT
Short for Roundtime.

## S

### Scrollback
History of text that can be scrolled back to view.

### Session
Connection session with the game server.

### SNI
Server Name Indication. TLS extension disabled for eAccess compatibility.

### Spirit
Spiritual energy resource. Tracked by vitals.spirit.

### SSL
Secure Sockets Layer. Legacy term, now TLS.

### Stamina
Physical energy resource. Tracked by vitals.stamina.

### Stream
Category of game output (main, room, combat, speech, etc.).

### Sync
Process of updating widgets to match current game state.

## T

### Tab
In `tabbedtext` widget, a separate pane within the widget.

### Tabbed Text
Widget type with multiple tabs, each showing different content.

### Terminal
Text-based interface environment.

### Theme
Collection of color definitions for consistent appearance.

### Ticket
Authentication token for game server connection.

### TLS
Transport Layer Security. Encryption for network connections.

### TOML
Tom's Obvious, Minimal Language. Configuration file format.

### Trigger
Automation that executes commands based on game output patterns.

### True Color
24-bit color (16.7 million colors). Also called "truecolor".

### TTS
Text-to-Speech. Audio output of game text.

## U

### Unicode
Character encoding standard supporting international characters and symbols.

### Update
Change to game state that triggers widget refresh.

## V

### Vitals
Character health statistics (health, mana, stamina, spirit).

### Viewport
Visible portion of scrollable content.

## W

### Widget
UI component displaying game information or accepting input.

### Widget Manager
Core component managing widget lifecycle and updates.

## X

### XML
eXtensible Markup Language. Format of game protocol.

## Z

### Z-Index
Layer order for overlapping widgets. Higher values display on top.

## See Also

- [Architecture](../architecture/README.md) - System design
- [Configuration](../configuration/README.md) - Settings reference
- [Widgets](../widgets/README.md) - Widget types

