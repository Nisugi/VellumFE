# VellumFE Companion Server Specification

## Overview

Add a companion server to VellumFE that allows mobile devices (or other clients) to connect and receive game feed data and send commands. VellumFE acts as a proxy between Lich and companion apps.

## Architecture

```
Mobile Device (iOS/Android/Web)
         │
         │ WebSocket (WSS)
         │ Port 9000 (configurable)
         │
         ▼
   ┌─────────────────────────────┐
   │       VellumFE (Proxy)      │
   │  ┌──────────────────────┐   │
   │  │  Terminal UI (TUI)   │   │  ← Main interface (unchanged)
   │  └──────────────────────┘   │
   │  ┌──────────────────────┐   │
   │  │  Companion Server    │   │  ← NEW: WebSocket server
   │  │  - Broadcasts events │   │
   │  │  - Receives commands │   │
   │  │  - Authentication    │   │
   │  └──────────────────────┘   │
   └──────────┬──────────────────┘
              │ TCP :8000
              ▼
         ┌────────┐
         │  Lich  │
         └───┬────┘
             │
             ▼
       Game Server
```

## Protocol Specification

### WebSocket Events (Server → Client)

All messages are JSON-encoded.

#### Vitals Update
```json
{
  "type": "vitals",
  "timestamp": 1634567890,
  "data": {
    "health": {"current": 326, "max": 326},
    "mana": {"current": 481, "max": 481},
    "stamina": {"current": 235, "max": 235},
    "spirit": {"current": 10, "max": 10}
  }
}
```

#### Text Output
```json
{
  "type": "text",
  "timestamp": 1634567890,
  "stream": "main",
  "content": "You are standing in a room.",
  "color": "#ffffff",
  "bold": false
}
```

#### Room Update
```json
{
  "type": "room",
  "timestamp": 1634567890,
  "title": "Town Square",
  "description": "You are standing in the town square.",
  "exits": ["north", "south", "east", "west"],
  "objects": ["bench", "fountain"],
  "players": ["Bob", "Alice"]
}
```

#### Status Update (RT, CT, Stun)
```json
{
  "type": "status",
  "timestamp": 1634567890,
  "roundtime": 3,
  "casttime": 0,
  "stunned": false
}
```

#### Compass Update
```json
{
  "type": "compass",
  "timestamp": 1634567890,
  "exits": ["north", "south", "east", "west", "up"]
}
```

#### Experience Update
```json
{
  "type": "experience",
  "timestamp": 1634567890,
  "level": 50,
  "current_exp": 1234567,
  "tnl": 50000
}
```

#### Indicator Update
```json
{
  "type": "indicator",
  "timestamp": 1634567890,
  "indicators": {
    "bleeding": 0,
    "poisoned": 0,
    "diseased": 0,
    "stunned": 0,
    "webbed": 0,
    "dead": 0,
    "kneeling": 0,
    "prone": 0,
    "sitting": 0,
    "standing": 1
  }
}
```

#### Active Effects Update
```json
{
  "type": "active_effects",
  "timestamp": 1634567890,
  "category": "ActiveSpells",
  "effects": [
    {
      "name": "Spirit Defense (103)",
      "duration": 3600,
      "remaining": 3540
    },
    {
      "name": "Elemental Defense (401)",
      "duration": 3600,
      "remaining": 3520
    }
  ]
}
```

#### Notification (Whispers, Deaths, Important Events)
```json
{
  "type": "notification",
  "timestamp": 1634567890,
  "priority": "high",
  "title": "Whisper from Bob",
  "content": "Bob whispers, 'Want to hunt?'",
  "sound": true
}
```

### Commands (Client → Server)

#### Send Game Command
```json
{
  "type": "command",
  "text": "look"
}
```

#### Authentication (First Message)
```json
{
  "type": "auth",
  "token": "your-secret-token-here",
  "client_id": "mobile-app-v1.0",
  "device": "iPhone 15 Pro"
}
```

#### Subscribe to Events (Optional Filtering)
```json
{
  "type": "subscribe",
  "events": ["vitals", "text", "room", "notifications"]
}
```

#### Ping (Keepalive)
```json
{
  "type": "ping"
}
```

Server responds with:
```json
{
  "type": "pong",
  "timestamp": 1634567890
}
```

## Implementation Plan

### Phase 1: Core Server (Week 1-2)

**Files to Create:**
- `src/companion_server.rs` - WebSocket server implementation
- `src/companion_protocol.rs` - Event serialization/deserialization

**Dependencies to Add:**
```toml
[dependencies]
tokio-tungstenite = "0.21"  # WebSocket server
serde_json = "1.0"          # JSON serialization (already have)
uuid = "1.0"                # Client session IDs
```

**Key Components:**

```rust
// src/companion_server.rs
pub struct CompanionServer {
    listener: Option<TcpListener>,
    clients: HashMap<String, CompanionClient>,
    config: CompanionServerConfig,
}

pub struct CompanionClient {
    id: String,
    stream: WebSocketStream<TcpStream>,
    authenticated: bool,
    subscriptions: HashSet<EventType>,
    device_info: Option<String>,
}

impl CompanionServer {
    pub fn new(config: CompanionServerConfig) -> Self;
    pub async fn start(&mut self) -> Result<(), Error>;
    pub fn broadcast_event(&mut self, event: CompanionEvent);
    pub fn try_recv_command(&mut self) -> Option<String>;
    pub fn handle_client_message(&mut self, client_id: &str, msg: Message);
}
```

**Config Integration:**

Add to `src/config.rs`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionServerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_companion_port")]
    pub port: u16,
    #[serde(default = "default_companion_bind")]
    pub bind_address: String,  // "127.0.0.1" or "0.0.0.0"
    #[serde(default)]
    pub require_auth: bool,
    #[serde(default)]
    pub auth_token: Option<String>,
    #[serde(default = "default_max_clients")]
    pub max_clients: usize,
}

fn default_companion_port() -> u16 { 9000 }
fn default_companion_bind() -> String { "127.0.0.1".to_string() }
fn default_max_clients() -> usize { 5 }
```

Add to `defaults/config.toml`:
```toml
[companion_server]
enabled = false
port = 9000
bind_address = "127.0.0.1"  # Localhost only by default (secure)
require_auth = true
auth_token = ""  # Generate on first run if empty
max_clients = 5
```

### Phase 2: Event Broadcasting (Week 2-3)

**Integration Points in `app.rs`:**

```rust
// In App struct:
pub struct App {
    // ... existing fields
    companion_server: Option<CompanionServer>,
}

// In handle_server_message():
ParsedElement::ProgressBar { id, value, max, text } => {
    self.window_manager.update_progress(&id, value, max, text);

    // Broadcast to companions
    if let Some(server) = &mut self.companion_server {
        if id == "health" || id == "mana" || id == "stamina" || id == "spirit" {
            server.broadcast_event(CompanionEvent::Vitals {
                timestamp: SystemTime::now(),
                data: self.get_current_vitals(),
            });
        }
    }
}

ParsedElement::Text { content, color, bold } => {
    self.window_manager.add_text(&stream, content, color, bold);

    // Broadcast to companions
    if let Some(server) = &mut self.companion_server {
        server.broadcast_event(CompanionEvent::Text {
            timestamp: SystemTime::now(),
            stream: stream.clone(),
            content: content.clone(),
            color: color.clone(),
            bold,
        });
    }
}

ParsedElement::RoomName { name } => {
    self.current_room.name = Some(name.clone());

    // Broadcast room update
    if let Some(server) = &mut self.companion_server {
        server.broadcast_event(CompanionEvent::Room {
            timestamp: SystemTime::now(),
            title: name,
            description: self.current_room.description.clone(),
            exits: self.current_room.exits.clone(),
            // ...
        });
    }
}

// In event loop (receiving commands from companions):
if let Some(server) = &mut self.companion_server {
    while let Some(command) = server.try_recv_command() {
        // Send to Lich (same as keyboard input)
        self.connection.send_command(&command);

        // Echo to main window
        self.window_manager.echo_command(&command);
    }
}
```

### Phase 3: Authentication & Security (Week 3)

**Features:**
- Token-based authentication
- Auto-generate token on first run
- Rate limiting (max commands per second)
- IP whitelist/blacklist
- TLS/SSL support (optional)

**Token Generation:**
```rust
fn generate_auth_token() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                            abcdefghijklmnopqrstuvwxyz\
                            0123456789";
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
```

**Display token on startup:**
```
[Companion Server] Started on 127.0.0.1:9000
[Companion Server] Auth Token: AbCd1234EfGh5678IjKl9012MnOp3456
[Companion Server] Use this token to connect mobile apps
```

### Phase 4: Testing & Documentation (Week 4)

**Test Client (CLI):**
Create `examples/companion_test_client.rs`:
```rust
// Simple test client that connects and displays events
#[tokio::main]
async fn main() {
    let ws = connect("ws://127.0.0.1:9000").await;

    // Send auth
    ws.send(json!({
        "type": "auth",
        "token": "your-token-here"
    }));

    // Listen for events
    while let Some(msg) = ws.next().await {
        println!("Received: {}", msg);
    }
}
```

**Documentation:**
- Update CLAUDE.md with companion server architecture
- Add COMPANION_APP.md (this file)
- Add example mobile app code snippets
- Add setup guide for remote access (VPN, port forwarding)

## Mobile App Implementation

### Minimal Viable Companion (Flutter)

**Features:**
- Connect to VellumFE server
- Display vitals (health, mana, stamina, spirit)
- Show main game text (scrolling)
- Command input field
- Basic notifications

**Screen Layout:**
```
┌─────────────────────────────┐
│ VellumFE Companion   [⚙️]   │  ← Top bar with settings
├─────────────────────────────┤
│ HP: 326/326  MP: 481/481    │  ← Vitals bar (always visible)
│ ST: 235/235  SP: 10/10      │
├─────────────────────────────┤
│                             │
│  [Room: Town Square]        │  ← Game text area (scrolling)
│                             │
│  You are standing in the    │
│  town square. The fountain  │
│  gurgles softly.            │
│                             │
│  > look                     │
│  You see nothing special.   │
│                             │
│                             │
├─────────────────────────────┤
│ RT: 3s  CT: 0s  [🎯] [💀]   │  ← Status indicators
├─────────────────────────────┤
│ [look] [stand] [inv]  [📍]  │  ← Quick commands
├─────────────────────────────┤
│ > _                    [↵]  │  ← Command input
└─────────────────────────────┘
```

**Dependencies (pubspec.yaml):**
```yaml
dependencies:
  flutter:
    sdk: flutter
  web_socket_channel: ^2.4.0
  provider: ^6.0.5
  shared_preferences: ^2.2.0
  flutter_local_notifications: ^15.1.0
```

**Core Files:**

1. `lib/services/companion_service.dart` - WebSocket connection
2. `lib/models/game_state.dart` - State management
3. `lib/screens/main_screen.dart` - Main UI
4. `lib/screens/settings_screen.dart` - Connection settings
5. `lib/widgets/vitals_bar.dart` - Health/mana display
6. `lib/widgets/game_text.dart` - Scrolling text display
7. `lib/widgets/command_input.dart` - Command entry

**Basic Connection Example:**
```dart
class CompanionService {
  WebSocketChannel? _channel;
  final String serverUrl;
  final String authToken;

  Stream<CompanionEvent> get events => _channel!.stream
      .map((msg) => CompanionEvent.fromJson(jsonDecode(msg)));

  Future<void> connect() async {
    _channel = WebSocketChannel.connect(Uri.parse(serverUrl));

    // Send auth
    _channel!.sink.add(jsonEncode({
      'type': 'auth',
      'token': authToken,
      'client_id': 'flutter-companion-v1.0',
      'device': Platform.operatingSystem,
    }));
  }

  void sendCommand(String command) {
    _channel!.sink.add(jsonEncode({
      'type': 'command',
      'text': command,
    }));
  }
}
```

## Security Considerations

### Default Security Model (Safe)

**Out of the box:**
- Bind to `127.0.0.1` (localhost only)
- Require authentication (token)
- Can only connect from same machine

**Use case:** Testing, same-machine multi-window

### Local Network Access (Medium Risk)

**Config changes:**
```toml
[companion_server]
enabled = true
bind_address = "0.0.0.0"  # Allow LAN connections
require_auth = true
auth_token = "AbCd1234..."  # Generated token
```

**Firewall:** Allow port 9000 on local network only

**Use case:** Phone on same WiFi as gaming PC

### Remote Access (Higher Risk)

**Option 1: VPN (Recommended)**
- Use Tailscale, ZeroTier, or WireGuard
- VellumFE binds to localhost
- VPN makes localhost accessible remotely
- Most secure option

**Option 2: Port Forwarding + Auth**
- Forward router port 9000 → PC:9000
- **MUST** use strong auth token
- Consider adding TLS/SSL
- Consider rate limiting

**Option 3: Reverse Proxy (Advanced)**
- Use nginx/caddy with TLS termination
- Add additional authentication layer
- Best for multiple users

## Performance Considerations

### Bandwidth Usage

**Estimated per-hour:**
- Vitals updates (1/sec): ~100 KB/hr
- Text output (moderate): ~500 KB/hr
- Room updates: ~50 KB/hr
- **Total: ~650 KB/hr (~15 MB/day)**

**Optimizations:**
- Only send text for subscribed streams
- Compress repeated data (delta updates)
- Client-side throttling (max updates/sec)

### Battery Impact (Mobile)

**Factors:**
- WebSocket keepalive: Minimal
- Screen on time: High
- Notification checks: Low
- **Estimate: ~2-5% battery/hour** (screen on)

**Optimizations:**
- Background mode (screen off): Only critical events
- Configurable update rates
- WiFi vs cellular detection

## Future Enhancements

### Phase 2 Features (Post-MVP)

1. **Scripting Integration**
   - Broadcast script status
   - Start/stop scripts from mobile
   - Script output routing

2. **Map Display**
   - Visual room map
   - Auto-mapping
   - Path finding

3. **Enhanced Notifications**
   - Configurable triggers
   - Push notifications (APNs/FCM)
   - Sound/vibration patterns

4. **Multi-Character Support**
   - Connect to multiple VellumFE instances
   - Switch between characters
   - Aggregate view (all characters' vitals)

5. **Voice Commands**
   - Speech-to-text for commands
   - Text-to-speech for output
   - Hands-free gameplay

6. **Offline Mode**
   - Cache recent text/state
   - Queue commands when disconnected
   - Replay on reconnect

7. **Collaboration Features**
   - Share screen with party members
   - Collaborative mapping
   - Group notifications

## Compatibility

### VellumFE Versions
- **Minimum:** v0.1.0 (after companion server merge)
- **Recommended:** Latest stable

### Mobile Platforms
- **iOS:** 14.0+
- **Android:** 8.0+ (API 26+)
- **Web:** Modern browsers (Chrome, Safari, Firefox)

### Network Requirements
- **Minimum:** 56 Kbps (dialup speeds work)
- **Recommended:** 1 Mbps+
- **Latency:** <500ms (for responsive commands)

## Open Questions / Decisions Needed

1. **Should we support multiple simultaneous companions per VellumFE instance?**
   - Pros: Use phone + tablet simultaneously
   - Cons: Increased bandwidth, complexity
   - **Recommendation:** Yes, default limit 5 clients

2. **Should companions be able to see command history?**
   - Pros: Useful for resuming on different device
   - Cons: Privacy concerns (passwords, sensitive commands)
   - **Recommendation:** Optional, off by default

3. **Should we add a web-based companion (browser)?**
   - Pros: No app installation needed, works on any device
   - Cons: Less native feel, no push notifications
   - **Recommendation:** Yes, but Phase 2

4. **Should companion server be in main binary or separate process?**
   - Pros (main binary): Simpler deployment, shared state
   - Cons (main binary): Slight performance impact
   - **Recommendation:** Main binary with config to disable

5. **What happens if VellumFE exits while companions connected?**
   - Grace period to reconnect?
   - Notification to companions?
   - **Recommendation:** Send "server_shutdown" event, 30s grace period

## Testing Checklist

### Server Side
- [ ] WebSocket server starts on configured port
- [ ] Multiple clients can connect simultaneously
- [ ] Authentication required and validated
- [ ] Invalid auth tokens rejected
- [ ] Events broadcast to all connected clients
- [ ] Commands from companions sent to Lich
- [ ] Server handles client disconnections gracefully
- [ ] Rate limiting prevents command spam
- [ ] Config changes reload without restart

### Client Side
- [ ] Connects to server successfully
- [ ] Reconnects after network interruption
- [ ] Displays vitals accurately
- [ ] Shows game text in correct order
- [ ] Commands send successfully
- [ ] Notifications appear for important events
- [ ] Works on cellular and WiFi
- [ ] Battery usage acceptable
- [ ] UI responsive on various screen sizes

### Integration
- [ ] No performance impact on main VellumFE UI
- [ ] Commands from companion echo in VellumFE
- [ ] State stays synchronized
- [ ] Logging captures companion activity
- [ ] Works with existing Lich setup

## References

- WebSocket Protocol: https://datatracker.ietf.org/doc/html/rfc6455
- tokio-tungstenite: https://docs.rs/tokio-tungstenite/
- Flutter WebSocket: https://pub.dev/packages/web_socket_channel
- Tailscale (VPN): https://tailscale.com/

## Changelog

- 2025-10-13: Initial specification created
