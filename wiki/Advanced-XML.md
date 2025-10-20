# XML Protocol Reference

VellumFE parses GemStone IV's XML protocol to extract game data. This guide covers the XML protocol in depth for advanced users who want to understand how game communication works.

## Protocol Overview

GemStone IV sends game output as XML-formatted text:

```xml
<pushStream id="speech"/>
<preset id='speech'>Adventurer says, "Hello!"</preset>
<popStream/>
```

VellumFE's parser processes this XML and:
1. Extracts text content
2. Applies styling (colors, bold)
3. Routes to appropriate streams/windows
4. Updates game state (vitals, timers, etc.)

## XML Tag Categories

### Stream Control Tags

Control where text is routed.

**pushStream:**
```xml
<pushStream id="speech"/>
```
Pushes "speech" stream onto stack. Text following this goes to "speech" stream.

**popStream:**
```xml
<popStream/>
```
Pops current stream, returns to previous stream.

### Styling Tags

Apply colors and text formatting.

**preset:**
```xml
<preset id='speech'>Player says, "Hello!"</preset>
```
Applies preset color defined in config.

**d:**
```xml
<d>Some text</d>
```
Resets to default color.

**pushBold / popBold:**
```xml
<pushBold/>Important!<popBold/>
```
Makes text bold.

**color:**
```xml
<color fg='#ff0000' bg='#000000'>Red text</color>
```
Applies custom colors.

### Game State Tags

Update vitals, timers, and other state.

**progressBar:**
```xml
<progressBar id='health' value='50' text='50/100'/>
```
Updates progress bar widget.

**roundTime:**
```xml
<roundTime value='1735689600'/>
```
Sets roundtime countdown (Unix timestamp).

**castTime:**
```xml
<castTime value='1735689603'/>
```
Sets cast time countdown (Unix timestamp).

**dialogData:**
```xml
<dialogData id='minivitals'>
  <progressBar id='health' value='50'/>
  <progressBar id='mana' value='75'/>
</dialogData>
```
Container for multiple state updates.

### Link Tags

Make game objects clickable.

**a (anchor):**
```xml
<a exist="12345" noun="orc">a big orc</a>
```
Creates clickable link for game object.

**menuResponse:**
```xml
<menuResponse id="12345">
  <mi coord="2524,2061" noun="sword"/>
  <mi coord="2524,2062" noun="shield"/>
</menuResponse>
```
Provides menu data for clickable link context menu.

### Prompt Tags

Game command prompt.

**prompt:**
```xml
<prompt time='1735689600'>&gt;</prompt>
```
Command prompt character (typically `>`).

### Special Tags

**compDef:**
```xml
<compDef id='room desc'></compDef>
```
Component definition (used for UI layout info).

**component:**
```xml
<component id='room desc'>You see a dark forest.</component>
```
Component content.

**clearStream:**
```xml
<clearStream id='main'/>
```
Clears stream content (not fully implemented in VellumFE).

**output:**
```xml
<output class="mono"/>
```
Changes output mode (mono = monospace).

## Parser State Machine

VellumFE's parser maintains several stacks:

### Color Stack

Tracks nested color tags:

```xml
<color fg='#ff0000'>
  Red text
  <color fg='#00ff00'>
    Green text
  </color>
  Back to red
</color>
```

Parser pushes/pops colors to maintain correct styling through nesting.

### Preset Stack

Similar to color stack, for preset tags:

```xml
<preset id='speech'>
  Speech color
  <preset id='whisper'>
    Whisper color
  </preset>
  Back to speech color
</preset>
```

### Bold Stack

Tracks bold state:

```xml
<pushBold/>
  Bold text
  <pushBold/>
    Still bold (nested)
  <popBold/>
  Still bold
<popBold/>
Not bold
```

### Style Stack

Tracks text style attributes (underline, italic, etc.):

```xml
<style id='underline'>Underlined text</style>
```

## Tag Processing

### Self-Closing Tags

Tags with no content:

```xml
<roundTime value='1735689600'/>
<popStream/>
<pushBold/>
```

### Paired Tags

Tags with opening and closing:

```xml
<preset id='speech'>Content</preset>
<pushStream id='main'>Content</pushStream>
```

### Attributes

Tags can have attributes:

```xml
<progressBar id='health' value='50' text='50/100' left='0' top='0'/>
```

**Common attributes:**
- `id` - Identifier
- `value` - Numeric value
- `text` - Display text
- `fg` / `bg` - Foreground/background colors
- `exist` - Object ID for clickable links
- `noun` - Object name

## HTML Entity Decoding

XML may contain HTML entities:

```xml
&lt;   → <
&gt;   → >
&amp;  → &
&quot; → "
&apos; → '
```

VellumFE automatically decodes these.

**Example:**
```xml
<preset id='room'>&lt;You see a sign that says "Welcome"&gt;</preset>
```

Displays: `<You see a sign that says "Welcome">`

## Stream Routing Flow

### Example Flow

**Game sends:**
```xml
<pushStream id='speech'/>
<preset id='speech'>Adventurer says, "Hello!"</preset>
<popStream/>
```

**VellumFE processes:**
1. Parses `<pushStream id='speech'/>`
   - Pushes "speech" onto stream stack
   - Current stream: "speech"

2. Parses `<preset id='speech'>`
   - Looks up "speech" preset in config
   - Finds `fg = "#53a684"`
   - Applies green color

3. Processes text: `Adventurer says, "Hello!"`
   - Routes to "speech" stream
   - Sent to windows subscribing to "speech"
   - Text colored green

4. Parses `</preset>`
   - Pops preset color
   - Returns to default color

5. Parses `<popStream/>`
   - Pops "speech" stream
   - Returns to previous stream (typically "main")

## Vitals Updates

### progressBar Tag

**Format:**
```xml
<progressBar id='ID' value='CURRENT' text='DISPLAY' left='X' top='Y'/>
```

**Attributes:**
- `id` - Widget identifier (health, mana, etc.)
- `value` - Current value (0-100 typically)
- `text` - Display text (e.g., "50/100")
- `left` / `top` - Position (ignored by VellumFE)

**Example:**
```xml
<progressBar id='health' value='50' text='50/100'/>
```

Updates health bar to 50% with text "50/100".

### dialogData Container

Multiple progress bars can be sent together:

```xml
<dialogData id='minivitals'>
  <progressBar id='health' value='50'/>
  <progressBar id='mana' value='75'/>
  <progressBar id='stamina' value='100'/>
  <progressBar id='spirit' value='90'/>
</dialogData>
```

VellumFE extracts each progressBar and updates corresponding widgets.

### Special Cases

**Encumbrance:**
```xml
<progressBar id='encumlevel' value='25'/>
```

VellumFE auto-colors based on value:
- 0-25: Green
- 26-50: Yellow
- 51-75: Brown
- 76-100: Red

**Mind state:**
```xml
<progressBar id='mindState' value='100' text='clear as a bell'/>
```

Shows custom text instead of numbers.

**Blood points:**
```xml
<dialogData id='BetrayerPanel'>
  <label value='Blood Points: 15' id='lblBPs'/>
</dialogData>
```

VellumFE parses "Blood Points: XX" and updates bloodpoints widget.

## Countdown Timers

### roundTime Tag

**Format:**
```xml
<roundTime value='TIMESTAMP'/>
```

**Attributes:**
- `value` - Unix timestamp when roundtime ends

**Example:**
```xml
<roundTime value='1735689605'/>
```

If current time is 1735689600, roundtime is 5 seconds.

### castTime Tag

**Format:**
```xml
<castTime value='TIMESTAMP'/>
```

Same as roundTime but for casting.

### Stun Timer

Not directly sent via XML tag. Typically inferred from game messages or sent via progressBar:

```xml
<progressBar id='stun' value='3' text='3 seconds'/>
```

## Clickable Links

### a (Anchor) Tag

**Format:**
```xml
<a exist='OBJECT_ID' noun='OBJECT_NAME'>display text</a>
```

**Attributes:**
- `exist` - Unique object ID
- `noun` - Object name (for commands)

**Example:**
```xml
You see <a exist='12345' noun='orc'>a big orc</a>.
```

Clicking "orc" or "big" opens context menu for object 12345.

### menuResponse Tag

After requesting menu (via `_menu #12345`), game sends:

```xml
<menuResponse id='12345'>
  <mi coord='2524,2061'/>
  <mi coord='2524,2062' noun='sword'/>
  <mi coord='2525,2073' noun='shield'/>
</menuResponse>
```

**Attributes:**
- `id` - Object ID (matches `exist` from anchor)
- `coord` - Command coordinate in cmdlist1.xml
- `noun` - Secondary noun for `%` substitution

VellumFE looks up each coord in cmdlist1.xml to build context menu.

### Command Substitution

Commands may contain placeholders:

- `#` - Replaced with exist ID
- `@` - Replaced with noun
- `%` - Replaced with secondary noun (from `<mi noun='...'/>`)

**Example cmdlist1.xml entry:**
```
command: look at # @
```

**With exist='12345', noun='orc':**
```
look at #12345 orc
```

## Prompt Processing

### prompt Tag

**Format:**
```xml
<prompt time='TIMESTAMP'>PROMPT_TEXT</prompt>
```

**Example:**
```xml
<prompt time='1735689600'>&gt;</prompt>
```

VellumFE displays `>` with prompt colors.

**Prompt indicators:**
- `>` - Normal prompt
- `R>` - Roundtime active
- `C>` - Casting
- `S>` - Stunned
- `L>` - (varies by game)
- `M>` - (varies by game)

Each indicator colored per `ui.prompt_colors` config.

## Parsing Edge Cases

### Malformed XML

VellumFE attempts to handle malformed XML gracefully:

**Unclosed tags:**
```xml
<preset id='speech'>Text without closing tag
```

Parser may flush text buffer and continue.

**Unknown tags:**
```xml
<unknownTag>Content</unknownTag>
```

Parser ignores unknown tags, processes content.

### Nested Tags

**Color within color:**
```xml
<color fg='#ff0000'>Red <color fg='#00ff00'>Green</color> Red again</color>
```

Parser maintains color stack for correct nesting.

**Preset within preset:**
```xml
<preset id='speech'>Speech <preset id='whisper'>Whisper</preset> Speech</preset>
```

Works correctly via preset stack.

### Text Buffer Flushing

Parser flushes text buffer when:
- Opening color/preset tag
- Closing color/preset tag
- Stream push/pop
- Line ending

This ensures correct styling boundaries.

## Debugging XML

### Enable Debug Logs

Debug logs show XML parsing:

```
~/.vellum-fe/debug.log
```

**Look for:**
```
DEBUG Received: <pushStream id='speech'/>
DEBUG Parsed: StreamPush("speech")
DEBUG Current stream: speech
```

### Common Issues

**Text not colored:**
- Check preset defined in config
- Verify preset ID matches XML
- Check debug log for "Unknown preset" warnings

**Text not routed:**
- Check stream push/pop balance
- Verify window subscribes to stream
- Check debug log for stream stack state

**Timers not updating:**
- Check timestamp format (Unix epoch)
- Verify widget exists (roundtime, casttime)
- Check debug log for parsing errors

## XML Protocol Extensions

VellumFE supports standard GemStone IV XML protocol. Some features may be game-specific or server-specific.

**Supported:**
- Stream routing (pushStream/popStream)
- Color/styling (preset, color, bold)
- Vitals (progressBar)
- Timers (roundTime, castTime)
- Links (anchor, menuResponse)
- Prompts (prompt)

**Partially supported:**
- Dialog data (limited to specific IDs)
- Components (basic support)

**Not supported:**
- Graphics/images
- Sound via XML (use highlight sounds instead)
- Complex dialog boxes (only specific IDs)

## See Also

- [Advanced Streams](Advanced-Streams.md) - Stream routing details
- [Configuration](Configuration.md) - Preset configuration
- [Window Types](Window-Types.md) - Widget types
- [Troubleshooting](Troubleshooting.md) - Parsing issues
