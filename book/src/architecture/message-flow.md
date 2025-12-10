# Message Flow

This document traces data flow through VellumFE from network to screen and from keyboard to game server.

## Server → UI Flow

### Complete Flow Diagram

```
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│   Network    │───▶│    Parser    │───▶│   Message    │
│   (TCP/TLS)  │    │   (XML)      │    │  Processor   │
└──────────────┘    └──────────────┘    └──────────────┘
                                               │
                    ┌──────────────────────────┼──────────────────────────┐
                    │                          ▼                          │
              ┌─────┴─────┐            ┌──────────────┐            ┌──────┴─────┐
              │ GameState │            │   UiState    │            │  Windows   │
              │  (vitals, │            │  (streams,   │            │  (text     │
              │   room)   │            │   focus)     │            │   content) │
              └───────────┘            └──────────────┘            └────────────┘
                                               │
                                               ▼
                                       ┌──────────────┐
                                       │   Frontend   │
                                       │   Render     │
                                       └──────────────┘
```

### Step-by-Step Processing

#### 1. Network Receives Data

```rust
ServerMessage::Text(raw_xml)
// Example: "<pushBold/>A goblin<popBold/> attacks you!"
```

The network task runs asynchronously, sending messages via channel.

#### 2. Parser Extracts Elements

```rust
let elements: Vec<ParsedElement> = parser.parse(&raw_xml);

// Result:
// [
//   Text { content: "A goblin", bold: true, span_type: Monsterbold, ... },
//   Text { content: " attacks you!", bold: false, span_type: Normal, ... },
// ]
```

The parser maintains state (color stacks, current stream) across calls.

#### 3. MessageProcessor Routes to State

```rust
for element in elements {
    message_processor.process(element, &mut game_state, &mut ui_state);
}

// Processing logic:
match element {
    ParsedElement::Text { content, stream, fg_color, bg_color, bold, span_type, link_data } => {
        // Find window for this stream
        if let Some(window) = ui_state.windows.get_mut(&stream) {
            if let Some(text_content) = &mut window.text_content {
                // Create styled line
                let line = StyledLine::new(segments);
                text_content.add_line(line);  // Increments generation
            }
        }
    }

    ParsedElement::ProgressBar { id, value, max, text } => {
        // Update vitals
        match id.as_str() {
            "health" => game_state.vitals.health = (value, max),
            "mana" => game_state.vitals.mana = (value, max),
            // ...
        }
    }

    ParsedElement::Compass { directions } => {
        game_state.exits = directions;
    }

    // ... handle 30+ element types
}
```

#### 4. State Changes Detected

The sync system detects changes via generation counters:

```rust
// Each text_content tracks changes
pub struct TextContent {
    pub lines: VecDeque<StyledLine>,
    pub generation: u64,  // Increments on every add_line()
}
```

#### 5. Frontend Syncs and Renders

```rust
// In render loop:
sync_text_windows(&app.ui_state, &mut widget_manager);
sync_progress_bars(&app.game_state, &mut widget_manager);
sync_compass_widgets(&app.game_state, &mut widget_manager);
// ... more sync functions

// Render all widgets
terminal.draw(|frame| {
    for (name, widget) in &widget_manager.text_windows {
        frame.render_widget(&*widget, widget.rect());
    }
    // ... render other widget types
})?;
```

## User Input → Game Flow

### Complete Flow Diagram

```
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│   Frontend   │───▶│   Keybind    │───▶│    Core      │
│   (events)   │    │   Dispatch   │    │   Commands   │
└──────────────┘    └──────────────┘    └──────────────┘
                                               │
                          ┌────────────────────┴────────────────────┐
                          ▼                                         ▼
                   ┌──────────────┐                          ┌──────────────┐
                   │  UI Action   │                          │ Game Command │
                   │ (open menu,  │                          │  (send to    │
                   │  scroll)     │                          │   server)    │
                   └──────────────┘                          └──────────────┘
```

### Keybind Processing Layers

Keybinds are processed in priority order:

```rust
// Layer 1: Global keybinds (always active)
if let Some(action) = config.global_keybinds.match_key(&key) {
    return handle_global_action(action);
}

// Layer 2: Menu keybinds (in priority windows)
if has_priority_window(&ui_state.input_mode) {
    if let Some(action) = config.menu_keybinds.match_key(&key, context) {
        return handle_menu_action(action);
    }
}

// Layer 3: User keybinds (game mode)
if let Some(action) = config.keybinds.get(&key_string) {
    return handle_user_action(action);
}

// Layer 4: Default character input
command_input.insert_char(key.char);
```

### Action Types

| Action Type | Example | Effect |
|-------------|---------|--------|
| **UI Action** | Open menu, scroll | Modifies UiState |
| **Game Command** | Send "look" | Sent to server |
| **Macro** | Multi-command sequence | Multiple server sends |
| **Internal** | Change layout | Config/layout change |

## Event Loop

### Main Loop Structure

```rust
pub fn run(config: Config) -> Result<()> {
    // 1. Initialize
    let runtime = tokio::runtime::Builder::new_multi_thread().build()?;
    let mut app = AppCore::new(config)?;
    let mut frontend = TuiFrontend::new()?;

    // 2. Start network tasks
    let (server_tx, server_rx) = mpsc::unbounded_channel();
    let (command_tx, command_rx) = mpsc::unbounded_channel();
    runtime.spawn(network_task(server_tx, command_rx));

    // 3. Main event loop
    loop {
        // Poll for user input (non-blocking)
        let events = frontend.poll_events()?;

        for event in events {
            match event {
                FrontendEvent::Key(key) => {
                    if app.handle_key_event(key, &command_tx)? == ControlFlow::Break {
                        return Ok(());
                    }
                }
                FrontendEvent::Mouse(mouse) => {
                    app.handle_mouse_event(mouse)?;
                }
                FrontendEvent::Resize(w, h) => {
                    app.handle_resize(w, h)?;
                }
            }
        }

        // Process server messages (non-blocking drain)
        while let Ok(msg) = server_rx.try_recv() {
            match msg {
                ServerMessage::Text(text) => {
                    app.process_server_message(&text)?;
                }
                ServerMessage::Connected => { ... }
                ServerMessage::Disconnected => { ... }
            }
        }

        // Render frame
        frontend.render(&mut app)?;

        // Sleep to maintain frame rate (~60 FPS)
        std::thread::sleep(Duration::from_millis(16));
    }
}
```

### Async Network Integration

Network I/O runs in separate Tokio tasks:

```rust
async fn reader_task(
    stream: TcpStream,
    server_tx: UnboundedSender<ServerMessage>,
) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                server_tx.send(ServerMessage::Disconnected).ok();
                break;
            }
            Ok(_) => {
                server_tx.send(ServerMessage::Text(line.clone())).ok();
            }
            Err(_) => break,
        }
    }
}

async fn writer_task(
    mut stream: TcpStream,
    mut command_rx: UnboundedReceiver<String>,
) {
    while let Some(cmd) = command_rx.recv().await {
        if stream.write_all(cmd.as_bytes()).await.is_err() {
            break;
        }
    }
}
```

## Detailed Scenarios

### Scenario: Monster Attack Message

```
Server sends: "<pushBold/>A massive troll<popBold/> swings at you!"

1. Network Task
   └─▶ Receives bytes, sends ServerMessage::Text(...)

2. Main Loop
   └─▶ Receives from channel, calls process_server_message()

3. Parser
   └─▶ Parses XML tags:
       • <pushBold/> → Push bold state
       • "A massive troll" → Text with bold=true, span_type=Monsterbold
       • <popBold/> → Pop bold state
       • " swings at you!" → Text with bold=false
   └─▶ Returns Vec<ParsedElement>

4. MessageProcessor
   └─▶ For each Text element:
       • Applies highlight patterns
       • Routes to "main" stream
       • Adds to main window's TextContent
       • Increments generation counter

5. Sync Functions
   └─▶ Detects generation change
   └─▶ Copies new line to TextWindow widget

6. Frontend Render
   └─▶ Renders TextWindow with new content
   └─▶ Bold "A massive troll" in monsterbold color
```

### Scenario: User Types Command

```
User types: "attack troll" and presses Enter

1. Frontend
   └─▶ Captures key events: 'a', 't', 't', ..., Enter
   └─▶ Sends FrontendEvent::Key for each

2. Input Router (for each key)
   └─▶ Not a keybind → Insert into command input

3. On Enter
   └─▶ Get command text: "attack troll"
   └─▶ Check for client command (starts with '.')
   └─▶ Not client command → Send to server
   └─▶ command_tx.send("attack troll\n")

4. Writer Task
   └─▶ Receives from channel
   └─▶ Writes to TCP stream

5. Meanwhile
   └─▶ Command echoed to main window
   └─▶ Server response arrives via reader task
```

### Scenario: Progress Bar Update

```
Server sends: <progressBar id='health' value='75' text='health 375/500'/>

1. Parser
   └─▶ Creates ParsedElement::ProgressBar {
         id: "health",
         value: 375,
         max: 500,
         text: "health 375/500"
       }

2. MessageProcessor
   └─▶ Updates game_state.vitals.health = (375, 500)

3. Sync Functions
   └─▶ sync_progress_bars() reads vitals
   └─▶ Updates ProgressBar widget

4. Frontend
   └─▶ Renders bar at 75% filled
```

### Scenario: Room Change

```
Server sends multiple elements for room transition:
1. <clearStream id='room'/>
2. <nav rm='12345'/>
3. <streamWindow id='room' subtitle='Town Square'/>
4. <component id='room desc'>A bustling town square...</component>
5. <compass><dir value='n'/><dir value='e'/><dir value='out'/></compass>

Processing:

1. ClearStream
   └─▶ Clears room window content

2. RoomId
   └─▶ game_state.room_id = Some("12345")

3. StreamWindow
   └─▶ game_state.room_name = Some("Town Square")
   └─▶ Updates window title

4. Component
   └─▶ Adds description to room window

5. Compass
   └─▶ game_state.exits = ["n", "e", "out"]
   └─▶ Compass widget updates

All synced and rendered in next frame.
```

## Channel Communication

### Channel Types

| Channel | Direction | Purpose |
|---------|-----------|---------|
| `server_tx` | Network → Main | Server messages |
| `command_tx` | Main → Network | Game commands |

### Message Types

```rust
pub enum ServerMessage {
    Text(String),       // Server XML data
    Connected,          // Connection established
    Disconnected,       // Connection lost
    Error(String),      // Network error
}
```

## Error Handling

### Network Errors

```rust
// In reader task
match reader.read_line(&mut line).await {
    Ok(0) => {
        // EOF - server closed connection
        server_tx.send(ServerMessage::Disconnected).ok();
    }
    Err(e) => {
        // Read error
        server_tx.send(ServerMessage::Error(e.to_string())).ok();
    }
}
```

### Parse Errors

Parse errors are logged but don't crash:

```rust
// Invalid XML is skipped
if let Err(e) = parser.parse_chunk(&data) {
    tracing::warn!("Parse error: {}", e);
    // Continue processing
}
```

### State Errors

Missing windows are handled gracefully:

```rust
// If window doesn't exist, message is silently dropped
if let Some(window) = ui_state.windows.get_mut(&stream) {
    // Process
}
```

## See Also

- [Parser Protocol](./parser-protocol.md) - XML parsing details
- [Widget Sync](./widget-sync.md) - Generation-based sync
- [Performance](./performance.md) - Optimization strategies

