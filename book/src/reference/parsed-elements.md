# Parsed Elements

Reference of all ParsedElement variants from the game protocol parser.

## Overview

ParsedElements are the structured output of the XML parser. Each variant represents a specific type of game data.

## Text Elements

### Text

Plain text content.

```rust
ParsedElement::Text(String)
```

**Source**: Untagged text between XML elements.

### StyledText

Text with formatting.

```rust
ParsedElement::StyledText {
    text: String,
    bold: bool,
    preset: Option<String>,
}
```

**Source**: `<pushBold/>`, `<popBold/>`, `<preset id="..."/>`

## Room Elements

### RoomName

Current room name.

```rust
ParsedElement::RoomName(String)
```

**Source**: `<roomName>...</roomName>`

### RoomDesc

Room description text.

```rust
ParsedElement::RoomDesc(String)
```

**Source**: `<roomDesc>...</roomDesc>`

### RoomObjects

Objects in the room.

```rust
ParsedElement::RoomObjects(String)
```

**Source**: `<roomObjs>...</roomObjs>`

### RoomPlayers

Players in the room.

```rust
ParsedElement::RoomPlayers(String)
```

**Source**: `<roomPlayers>...</roomPlayers>`

### RoomExits

Available exits.

```rust
ParsedElement::RoomExits(String)
```

**Source**: `<roomExits>...</roomExits>`

## Vitals Elements

### Vitals

Character vital statistics.

```rust
ParsedElement::Vitals {
    health: Option<u8>,
    mana: Option<u8>,
    stamina: Option<u8>,
    spirit: Option<u8>,
}
```

**Source**: `<progressBar id="..." value="..."/>`

Progress bar IDs:
- `health`, `manapoints`, `stamina`, `spirit`
- `encumlevel`, `mindState`, `nextLvlPB`

## Timing Elements

### Roundtime

Action roundtime.

```rust
ParsedElement::Roundtime(u32)
```

**Source**: `<roundTime value="..."/>` or `Roundtime: N sec`

### CastTime

Spell casting time.

```rust
ParsedElement::CastTime(u32)
```

**Source**: `<castTime value="..."/>`

## Status Elements

### Indicator

Status indicator change.

```rust
ParsedElement::Indicator {
    id: String,
    visible: bool,
}
```

**Source**: `<indicator id="..." visible="y|n"/>`

Common indicator IDs:
- `IconHIDDEN`, `IconSTUNNED`, `IconWEBBED`
- `IconPRONE`, `IconKNEELING`, `IconSITTING`
- `IconBLEEDING`, `IconPOISONED`, `IconDISEASED`
- `IconINVISIBLE`, `IconDEAD`

### Stance

Combat stance.

```rust
ParsedElement::Stance(String)
```

**Source**: `<stance id="..."/>`

Values: `offensive`, `forward`, `neutral`, `guarded`, `defensive`

## Navigation Elements

### Compass

Available directions.

```rust
ParsedElement::Compass {
    directions: Vec<String>,
}
```

**Source**: `<compass><dir value="..."/>...</compass>`

Direction values:
- `n`, `s`, `e`, `w` (cardinal)
- `ne`, `nw`, `se`, `sw` (diagonal)
- `up`, `down`, `out`

## Hand Elements

### LeftHand

Item in left hand.

```rust
ParsedElement::LeftHand {
    noun: String,
    name: String,
    exist: Option<String>,
}
```

**Source**: `<left exist="..." noun="...">...</left>`

### RightHand

Item in right hand.

```rust
ParsedElement::RightHand {
    noun: String,
    name: String,
    exist: Option<String>,
}
```

**Source**: `<right exist="..." noun="...">...</right>`

### Spell

Prepared spell.

```rust
ParsedElement::Spell(String)
```

**Source**: `<spell>...</spell>`

## Stream Elements

### PushStream

Start of named stream.

```rust
ParsedElement::PushStream(String)
```

**Source**: `<pushStream id="..."/>`

### PopStream

End of current stream.

```rust
ParsedElement::PopStream
```

**Source**: `<popStream/>`

### StreamContent

Content within a stream.

```rust
ParsedElement::StreamContent {
    stream: String,
    content: String,
}
```

## Prompt Elements

### Prompt

Game prompt.

```rust
ParsedElement::Prompt {
    text: String,
    time: Option<String>,
}
```

**Source**: `<prompt time="...">...</prompt>`

## Link Elements

### Link

Clickable object link.

```rust
ParsedElement::Link {
    exist: String,
    noun: String,
    text: String,
}
```

**Source**: `<a exist="..." noun="...">...</a>`

### CommandLink

Clickable command link.

```rust
ParsedElement::CommandLink {
    cmd: String,
    text: String,
}
```

**Source**: `<d cmd="...">...</d>`

## Combat Elements

### Combat

Combat message.

```rust
ParsedElement::Combat {
    text: String,
}
```

**Source**: Text within combat stream.

### Damage

Damage dealt or received.

```rust
ParsedElement::Damage {
    amount: u32,
    target: Option<String>,
}
```

## Container Elements

### Container

Container contents.

```rust
ParsedElement::Container {
    id: String,
    name: String,
    contents: Vec<ContainerItem>,
}
```

### ContainerItem

Item within container.

```rust
ContainerItem {
    exist: String,
    noun: String,
    name: String,
}
```

## Spell Elements

### ActiveSpell

Currently active spell.

```rust
ParsedElement::ActiveSpell {
    name: String,
    duration: Option<u32>,
}
```

**Source**: `<spell>...</spell>` with duration

### SpellExpire

Spell expired notification.

```rust
ParsedElement::SpellExpire {
    name: String,
}
```

## System Elements

### Mode

Game mode change.

```rust
ParsedElement::Mode {
    mode: String,
}
```

**Source**: `<mode id="..."/>`

### Output

Output configuration.

```rust
ParsedElement::Output {
    class: String,
}
```

**Source**: `<output class="..."/>`

### ClearStream

Clear stream content.

```rust
ParsedElement::ClearStream(String)
```

**Source**: `<clearStream id="..."/>`

## Unknown Element

Unrecognized XML.

```rust
ParsedElement::Unknown {
    tag: String,
    content: String,
}
```

Preserves unhandled elements for debugging.

## Element Processing

Elements are processed in order:

```rust
for element in parser.parse(input) {
    match element {
        ParsedElement::RoomName(name) => {
            state.room.name = name;
        }
        ParsedElement::Vitals { health, .. } => {
            if let Some(h) = health {
                state.vitals.health = h;
            }
        }
        // ...
    }
}
```

## See Also

- [Parser Protocol](../architecture/parser-protocol.md)
- [Parser Extensions](../development/parser-extensions.md)
- [Stream IDs](./stream-ids.md)

