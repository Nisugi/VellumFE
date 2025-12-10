# XML Protocol Reference

Complete reference for GemStone IV's XML game protocol.

## Protocol Overview

GemStone IV sends game data as a mix of plain text and XML tags. The XML tags provide structured information about game state, while plain text contains narrative content.

### Example Stream

```xml
<prompt time="1699900000">&gt;</prompt>
<pushStream id="room"/>
<compDef id='room desc'/>
[Town Square Central]
<popStream/>
This is the center of town.
<compass>
<dir value="n"/>
<dir value="e"/>
<dir value="s"/>
<dir value="w"/>
</compass>
```

## Core Tags

### `<prompt>`

Indicates command prompt, includes server timestamp.

```xml
<prompt time="1699900000">&gt;</prompt>
```

| Attribute | Description |
|-----------|-------------|
| `time` | Unix timestamp from server |

### `<pushStream>` / `<popStream>`

Redirects content to a named stream.

```xml
<pushStream id="combat"/>
You attack the troll!
<popStream/>
```

Common stream IDs:
- `main` - Primary game output
- `room` - Room descriptions
- `combat` - Combat messages
- `speech` - Character speech
- `whisper` - Private messages
- `thoughts` - Mental communications
- `death` - Death messages
- `experience` - Experience gains
- `logons` - Login/logout notices
- `atmosphere` - Ambient messages

### `<clearStream>`

Clears a stream's content.

```xml
<clearStream id="room"/>
```

### `<component>`

Named data component for widgets.

```xml
<component id="room objs">a troll</component>
<component id="room players">Playername</component>
<component id="room exits">Obvious paths: north, south</component>
```

Common component IDs:
- `room objs` - Objects in room
- `room players` - Players in room
- `room exits` - Exits description
- `room desc` - Room description
- `room name` - Room title

### `<compDef>`

Component definition (marks component location).

```xml
<compDef id='room desc'/>
```

## Vitals Tags

### Health/Mana/Stamina/Spirit

```xml
<progressBar id="health" value="95"/>
<progressBar id="mana" value="100"/>
<progressBar id="stamina" value="87"/>
<progressBar id="spirit" value="100"/>
```

| Attribute | Description |
|-----------|-------------|
| `id` | Vital type |
| `value` | 0-100 percentage |

### Experience

```xml
<progressBar id="encumbrance" value="50"/>
<progressBar id="mindState" value="5"/>
```

### Encumbrance Levels

| Value | Description |
|-------|-------------|
| 0 | None |
| 1-20 | Light |
| 21-40 | Moderate |
| 41-60 | Heavy |
| 61-80 | Very Heavy |
| 81-100 | Encumbered |

## Timing Tags

### `<roundTime>`

Sets roundtime value.

```xml
<roundTime value="1699900005"/>
```

| Attribute | Description |
|-----------|-------------|
| `value` | Unix timestamp when RT ends |

### `<castTime>`

Sets casttime value.

```xml
<castTime value="1699900003"/>
```

## Navigation Tags

### `<compass>`

Navigation directions available.

```xml
<compass>
<dir value="n"/>
<dir value="ne"/>
<dir value="e"/>
<dir value="se"/>
<dir value="s"/>
<dir value="sw"/>
<dir value="w"/>
<dir value="nw"/>
<dir value="up"/>
<dir value="down"/>
<dir value="out"/>
</compass>
```

Direction values:
- `n`, `ne`, `e`, `se`, `s`, `sw`, `w`, `nw` - Cardinal/ordinal
- `up`, `down`, `out` - Vertical/exit

## Status Tags

### `<indicator>`

Boolean status indicator.

```xml
<indicator id="IconHIDDEN" visible="y"/>
<indicator id="IconSTUNNED" visible="n"/>
```

| Attribute | Description |
|-----------|-------------|
| `id` | Indicator identifier |
| `visible` | "y" or "n" |

Common indicators:
- `IconHIDDEN` - Hidden status
- `IconSTUNNED` - Stunned
- `IconBLEEDING` - Bleeding
- `IconPOISONED` - Poisoned
- `IconDISEASED` - Diseased
- `IconKNEELING` - Kneeling
- `IconPRONE` - Lying down
- `IconSITTING` - Sitting
- `IconSTANDING` - Standing
- `IconJOINED` - In a group
- `IconDEAD` - Dead

### `<left>` / `<right>`

Hand contents.

```xml
<left>a steel sword</left>
<right>a wooden shield</right>
```

Empty hands show as:
```xml
<left>Empty</left>
```

### `<spell>`

Currently prepared spell.

```xml
<spell>Fire Spirit</spell>
```

## Combat Tags

### `<a>` (Anchor/Link)

Clickable links in game text.

```xml
<a exist="123456" noun="troll">troll</a>
```

| Attribute | Description |
|-----------|-------------|
| `exist` | Object existence ID |
| `noun` | Base noun for commands |

### Target Information

```xml
<component id="target">a massive troll</component>
```

## Spell/Effect Tags

### Active Spells

Spell list updates:

```xml
<pushStream id="spells"/>
<clearStream id="spells"/>
<spell>Spirit Warding I (101)</spell> - 30 min remaining
<spell>Spirit Defense (103)</spell> - 25 min remaining
<popStream/>
```

### Active Effects

```xml
<dialogData id="ActiveSpells">
<progressBar id="spell_101" value="90" text="Spirit Warding I"/>
</dialogData>
```

## Inventory Tags

### Container Contents

```xml
<inv id="1234">
<item>a steel sword</item>
<item>3 silver coins</item>
</inv>
```

### Equipment

```xml
<pushStream id="inv"/>
Equipment:
  armor: some full leather
  weapon: a steel broadsword
<popStream/>
```

## Injury Tags

### Body Part Injuries

```xml
<component id="injury_head">0</component>
<component id="injury_neck">0</component>
<component id="injury_chest">1</component>
<component id="injury_back">0</component>
<component id="injury_leftArm">2</component>
<component id="injury_rightArm">0</component>
<component id="injury_leftHand">0</component>
<component id="injury_rightHand">0</component>
<component id="injury_abdomen">0</component>
<component id="injury_leftLeg">0</component>
<component id="injury_rightLeg">0</component>
<component id="injury_leftFoot">0</component>
<component id="injury_rightFoot">0</component>
<component id="injury_leftEye">0</component>
<component id="injury_rightEye">0</component>
<component id="injury_nsys">0</component>
```

Injury values:
- `0` - No injury
- `1` - Minor injury
- `2` - Moderate injury
- `3` - Severe injury

## Style Tags

### `<preset>`

Applies named color preset.

```xml
<preset id="speech">Someone says, "Hello"</preset>
<preset id="whisper">Someone whispers, "Hello"</preset>
```

Common presets:
- `speech` - Character speech
- `whisper` - Whispers
- `thought` - Mental communication
- `roomName` - Room title
- `roomDesc` - Room description
- `bold` - Bold text

### `<b>` / `<i>` / `<u>`

Basic text formatting.

```xml
<b>Bold text</b>
<i>Italic text</i>
<u>Underlined text</u>
```

### `<d cmd="...>`

Command suggestion (clickable).

```xml
<d cmd="look troll">look at the troll</d>
```

## Control Tags

### `<mode>`

Client mode setting.

```xml
<mode id="GAME"/>
```

### `<app>`

Application character information.

```xml
<app char="Charactername" game="GS4" />
```

### `<output>`

Output control.

```xml
<output class="mono"/>
```

## Special Tags

### `<resource>`

External resource reference.

```xml
<resource picture="12345"/>
```

### `<style>`

Inline style application.

```xml
<style id="roomName"/>
```

### `<nav>`

Navigation region.

```xml
<nav rm="12345"/>
```

## Parsing Considerations

### Entity Encoding

Special characters are XML-encoded:
- `&gt;` → `>`
- `&lt;` → `<`
- `&amp;` → `&`
- `&quot;` → `"`
- `&#39;` → `'`

### Nested Tags

Tags can nest:
```xml
<pushStream id="combat">
<preset id="thought">You sense <a exist="123" noun="creature">the creature</a></preset>
<popStream/>
```

### Unclosed Tags

Some tags are self-closing or have implicit closure:
```xml
<prompt time="123">&gt;</prompt>
<dir value="n"/>
<progressBar id="health" value="95"/>
```

## Implementation Notes

### Stream Management

- Push/pop streams form a stack
- Multiple streams can be active
- Text goes to all active streams
- Clear affects only named stream

### Component Updates

- Components update atomically
- Same ID replaces previous value
- Clear stream clears related components

### Timing Precision

- Timestamps are Unix seconds
- Client should convert to local time
- Roundtime countdown based on server time

## See Also

- [Parser Protocol](../architecture/parser-protocol.md) - Parsing implementation
- [Parsed Elements](../reference/parsed-elements.md) - Internal representation
- [Stream IDs](../reference/stream-ids.md) - Stream reference

