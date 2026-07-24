//! Lich WebUI bridge protocol types (schema 1).
//!
//! Lich 5.18+ runs a per-session HTTP/WebSocket server that scripts use to
//! register live UI pages. Instead of embedding a browser, VellumFE speaks
//! the "Level 2" native protocol: subscribe to a page over the WebSocket and
//! render its JSON component tree with native widgets.
//!
//! Wire format owner: lich-5 `lib/webui/protocol.rb` + `lib/webui/dsl.rb`.
//! Everything here is pure serde data - NO frontend imports.

use serde::{Deserialize, Serialize};

/// Parsed `<LichWebUI .../>` handshake reply (sent by Lich in response to
/// `;ui handshake`, as exactly one line on the game stream).
#[derive(Clone, Debug, PartialEq)]
pub struct WebUiHandshake {
    /// "ok", "disabled", or "stopped"
    pub status: String,
    pub port: u16,
    /// Landing page URL. Loopback for a local Lich
    /// ("http://127.0.0.1:51423/"); a reachable LAN/VPN address when Lich
    /// runs elsewhere, e.g. in a container ("http://192.168.86.4:8200/").
    pub url: String,
    /// Tokenized auth URL: "<url>auth?token=<64 hex>"
    pub auth: String,
    pub schema: u32,
}

impl WebUiHandshake {
    /// Host and port to dial, taken from the `url` attribute — Lich builds
    /// it as an address reachable from the FE (loopback normally, the LAN
    /// address for a containerized Lich). Falls back to loopback + the
    /// `port` attribute when `url` is absent or unparseable, and to the
    /// `port` attribute when the url carries no explicit port.
    pub fn endpoint(&self) -> (String, u16) {
        let fallback = || ("127.0.0.1".to_string(), self.port);
        let Some((_, rest)) = self.url.split_once("://") else {
            return fallback();
        };
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        if authority.is_empty() {
            return fallback();
        }
        match authority.rsplit_once(':') {
            Some((host, port)) if !host.is_empty() => match port.parse() {
                Ok(port) => (host.to_string(), port),
                Err(_) => fallback(),
            },
            _ => (authority.to_string(), self.port),
        }
    }

    /// Extracts the auth token from the auth URL. The token doubles as the
    /// value of the `lich_webui` cookie, so a native client can skip the
    /// HTTP /auth round-trip and present the cookie directly.
    pub fn token(&self) -> Option<&str> {
        let (_, token) = self.auth.split_once("token=")?;
        let token = token.split('&').next().unwrap_or(token);
        (!token.is_empty()).then_some(token)
    }
}

/// Session identity from the `hello` envelope.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct WebUiSession {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub game: String,
}

/// One registered page, from `hello`/`pages` envelopes.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct WebUiPageDescriptor {
    /// "script/page", e.g. "creaturebar/main"
    pub id: String,
    #[serde(default)]
    pub title: String,
    /// Owning script name
    #[serde(default)]
    pub script: String,
    /// Author embedding hint: "panel" (dock) or "window" (float); None = no opinion
    #[serde(default)]
    pub kind: Option<String>,
    /// Compact chromeless styling expected
    #[serde(default)]
    pub bare: bool,
    /// Preferred content size in CSS pixels [w, h]
    #[serde(default)]
    pub size: Option<[f32; 2]>,
}

/// Another local Lich session's WebUI endpoint (name/port only, no token).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct WebUiSibling {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub game: String,
    #[serde(default)]
    pub port: u16,
}

/// One node of a page's component tree.
///
/// Nodes are `{t, cid, ...attrs, children}`; every attribute is optional so
/// unknown/new fields never fail the parse (the envelope is versioned via
/// `schema_version`, additive changes are expected). Renderers match on `t`:
/// page, header, text, markdown, divider, button, text_input, password_input,
/// select, radio, checkbox, slider, number_input, log, progress, table,
/// expander, columns, col, tabs, tab, image, image_map.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct WebUiNode {
    /// Component type tag
    #[serde(default)]
    pub t: String,
    /// Component id, target for events ("button:3", "expander:5.text_input:0")
    #[serde(default)]
    pub cid: Option<String>,

    // page node
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub bare: Option<bool>,
    #[serde(default)]
    pub size: Option<[f32; 2]>,

    // header / text / markdown
    #[serde(default)]
    pub text: Option<String>,

    // labeled inputs + button + expander + tab
    #[serde(default)]
    pub label: Option<String>,

    // button
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub disabled: Option<bool>,
    #[serde(default)]
    pub confirm: Option<String>,

    // text_input / password_input / select / radio / slider / number_input
    /// Current value: string for text/select/radio, number for slider/number
    #[serde(default)]
    pub value: Option<serde_json::Value>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub options: Option<Vec<String>>,

    // checkbox
    #[serde(default)]
    pub checked: Option<bool>,

    // slider / number_input
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub step: Option<f64>,

    // log
    #[serde(default)]
    pub lines: Option<Vec<String>>,
    #[serde(default)]
    pub max_height: Option<f32>,

    // table
    #[serde(default)]
    pub headings: Option<Vec<String>>,
    #[serde(default)]
    pub rows: Option<Vec<Vec<String>>>,
    #[serde(default)]
    pub sortable: Option<bool>,
    #[serde(default)]
    pub clickable: Option<bool>,
    #[serde(default)]
    pub selected: Option<i64>,

    // expander
    #[serde(default)]
    pub open: Option<bool>,

    // columns
    #[serde(default)]
    pub compact: Option<bool>,
    #[serde(default)]
    pub weights: Option<Vec<f32>>,

    // grid (aligned matrix of `cell` children, row-major)
    /// Column count; rows = ceil(children / cols)
    #[serde(default)]
    pub cols: Option<u32>,

    // tabs
    #[serde(default)]
    pub vertical: Option<bool>,

    // image / image_map
    #[serde(default)]
    pub src: Option<String>,
    #[serde(default)]
    pub alt: Option<String>,
    #[serde(default)]
    pub scale: Option<f32>,
    /// image_map overlay boxes, in unscaled image-pixel coordinates
    #[serde(default)]
    pub markers: Option<Vec<WebUiMapMarker>>,
    /// Marker id to center the container on whenever it moves
    #[serde(default)]
    pub scroll_to: Option<String>,
    /// Page id a right-click opens as a supplemental window
    #[serde(default)]
    pub popup: Option<String>,
    #[serde(default)]
    pub popup_size: Option<[f32; 2]>,

    // containers (page, expander, columns/col, tabs/tab)
    #[serde(default)]
    pub children: Option<Vec<WebUiNode>>,
}

impl WebUiNode {
    pub fn children(&self) -> &[WebUiNode] {
        self.children.as_deref().unwrap_or(&[])
    }

    /// String view of `value` (text/select/radio inputs).
    pub fn value_str(&self) -> Option<&str> {
        self.value.as_ref().and_then(|v| v.as_str())
    }

    /// Numeric view of `value` (slider/number inputs).
    pub fn value_f64(&self) -> Option<f64> {
        self.value.as_ref().and_then(|v| v.as_f64())
    }
}

/// One image_map overlay box, in UNSCALED image-pixel coordinates
/// (positioned at coord * scale in display space, like the browser bundle).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct WebUiMapMarker {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub x1: f32,
    #[serde(default)]
    pub y1: f32,
    #[serde(default)]
    pub x2: f32,
    #[serde(default)]
    pub y2: f32,
    /// "current" (red glow circle) | "marker" (accent box) | "pin" (filled dot)
    #[serde(default)]
    pub kind: Option<String>,
    /// Tooltip text
    #[serde(default)]
    pub label: Option<String>,
}

/// Server -> client WebSocket envelopes. Unknown `type`s parse to `Unknown`
/// and are ignored (forward compatibility within a schema version).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WebUiServerMessage {
    Hello {
        #[serde(default)]
        schema_version: u32,
        #[serde(default)]
        session: WebUiSession,
        #[serde(default)]
        pages: Vec<WebUiPageDescriptor>,
        #[serde(default)]
        siblings: Vec<WebUiSibling>,
    },
    Pages {
        #[serde(default)]
        pages: Vec<WebUiPageDescriptor>,
    },
    Render {
        page: String,
        #[serde(default)]
        seq: u64,
        tree: WebUiNode,
    },
    /// Page is finished (script exited / page replaced).
    Close { page: String },
    /// OS-notification request (UI.notify).
    Notify {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        text: String,
    },
    /// User-facing notice: level is "info" | "warn" | "error".
    Notice {
        #[serde(default)]
        level: String,
        #[serde(default)]
        text: String,
    },
    #[serde(other)]
    Unknown,
}

/// Client -> server WebSocket messages.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WebUiClientMessage {
    Subscribe { page: String },
    Unsubscribe { page: String },
    /// Component interaction; `value` is component-specific (button: null,
    /// text_input: submitted string, checkbox: bool, table row click: index).
    Event {
        page: String,
        cid: String,
        value: serde_json::Value,
    },
}

/// Live state of one WebUI panel window (WindowContent::WebUi).
#[derive(Clone, Debug, Default)]
pub struct WebUiPanelContent {
    /// Page id this panel is bound to ("creaturebar/main")
    pub page_id: String,
    /// Display title (descriptor title, falls back to page id)
    pub title: String,
    /// Author embedding hint from the page descriptor, remembered here
    /// because the descriptor is gone from the registry by the time the
    /// page closes. `Some("panel")` = persistent (keep the window on page
    /// end, it auto-resumes); anything else = transient (auto-close).
    pub kind: Option<String>,
    /// Latest component tree; None until the first render arrives
    pub tree: Option<WebUiNode>,
    /// Last applied render sequence (stale/out-of-order renders are dropped)
    pub seq: u64,
    /// Bumped on every applied render so renderers can cache by generation
    pub generation: u64,
    /// True while the bridge WebSocket is up
    pub connected: bool,
    /// Set when the page ended (script killed); panel shows a notice and
    /// clears if the page re-registers
    pub ended: Option<String>,
}

impl WebUiPanelContent {
    pub fn new(page_id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            page_id: page_id.into(),
            title: title.into(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_token_extraction() {
        let hs = WebUiHandshake {
            status: "ok".into(),
            port: 51423,
            url: "http://127.0.0.1:51423/".into(),
            auth: "http://127.0.0.1:51423/auth?token=abc123def".into(),
            schema: 1,
        };
        assert_eq!(hs.token(), Some("abc123def"));
    }

    #[test]
    fn endpoint_prefers_url_authority() {
        // Local Lich: loopback url.
        let hs = WebUiHandshake {
            status: "ok".into(),
            port: 51423,
            url: "http://127.0.0.1:51423/".into(),
            auth: "http://127.0.0.1:51423/auth?token=abc".into(),
            schema: 1,
        };
        assert_eq!(hs.endpoint(), ("127.0.0.1".to_string(), 51423));

        // Containerized Lich: url carries the reachable LAN address, and its
        // port wins even if docker remapped it away from the `port` attr.
        let hs = WebUiHandshake {
            url: "http://192.168.86.4:8200/".into(),
            port: 51423,
            ..hs
        };
        assert_eq!(hs.endpoint(), ("192.168.86.4".to_string(), 8200));

        // Hostname without an explicit port: keep the port attribute.
        let hs = WebUiHandshake {
            url: "http://lich.tailnet.ts.net/".into(),
            port: 8200,
            ..hs
        };
        assert_eq!(hs.endpoint(), ("lich.tailnet.ts.net".to_string(), 8200));
    }

    #[test]
    fn endpoint_falls_back_to_loopback() {
        let hs = WebUiHandshake {
            status: "ok".into(),
            port: 51423,
            url: String::new(),
            auth: String::new(),
            schema: 1,
        };
        assert_eq!(hs.endpoint(), ("127.0.0.1".to_string(), 51423));

        let hs = WebUiHandshake {
            url: "not a url".into(),
            ..hs
        };
        assert_eq!(hs.endpoint(), ("127.0.0.1".to_string(), 51423));
    }

    #[test]
    fn handshake_token_missing() {
        let hs = WebUiHandshake {
            status: "disabled".into(),
            port: 0,
            url: String::new(),
            auth: String::new(),
            schema: 0,
        };
        assert_eq!(hs.token(), None);
    }

    #[test]
    fn parses_hello_envelope() {
        let raw = r#"{ "type": "hello", "schema_version": 1,
  "session": { "name": "Nisugi", "game": "GSIV" },
  "pages": [
    { "id": "creaturebar/main", "title": "Creature Bar", "script": "creaturebar",
      "kind": "panel", "bare": true, "size": [320, 90] },
    { "id": "map/map", "title": "Map: Nisugi", "kind": "window", "bare": true }
  ],
  "siblings": [ { "name": "Alt", "game": "GSIV", "port": 51999 } ] }"#;
        let msg: WebUiServerMessage = serde_json::from_str(raw).unwrap();
        let WebUiServerMessage::Hello { schema_version, session, pages, siblings } = msg else {
            panic!("expected hello, got {:?}", msg);
        };
        assert_eq!(schema_version, 1);
        assert_eq!(session.name, "Nisugi");
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].id, "creaturebar/main");
        assert_eq!(pages[0].kind.as_deref(), Some("panel"));
        assert!(pages[0].bare);
        assert_eq!(pages[0].size, Some([320.0, 90.0]));
        assert_eq!(siblings.len(), 1);
        assert_eq!(siblings[0].port, 51999);
    }

    #[test]
    fn parses_render_envelope_with_nested_tree() {
        // Shape produced by lich-5 Page#render_loop + Builder#emit
        let raw = r#"{ "type": "render", "page": "demo/demo", "seq": 7,
  "tree": { "t": "page", "title": "Demo", "bare": true, "size": [320, 90], "children": [
    { "t": "header", "cid": "header:0", "text": "Creatures" },
    { "t": "progress", "cid": "progress:1", "value": 0.4, "label": "kobold" },
    { "t": "button", "cid": "button:2", "label": "Attack", "variant": "danger", "disabled": false },
    { "t": "columns", "cid": "columns:3", "weights": [7.0, 3.0], "children": [
      { "t": "col", "cid": "columns:3.c0", "children": [
        { "t": "text", "cid": "columns:3.c0.text:0", "text": "left" } ] },
      { "t": "col", "cid": "columns:3.c1", "children": [] } ] },
    { "t": "table", "cid": "table:4", "headings": ["Name","HP"],
      "rows": [["kobold","12"],["orc","40"]], "clickable": true, "selected": 1 },
    { "t": "future_widget", "cid": "future_widget:5", "mystery_attr": 42 }
  ] } }"#;
        let msg: WebUiServerMessage = serde_json::from_str(raw).unwrap();
        let WebUiServerMessage::Render { page, seq, tree } = msg else {
            panic!("expected render, got {:?}", msg);
        };
        assert_eq!(page, "demo/demo");
        assert_eq!(seq, 7);
        assert_eq!(tree.t, "page");
        assert_eq!(tree.children().len(), 6);
        assert_eq!(tree.children()[1].value_f64(), Some(0.4));
        assert_eq!(tree.children()[2].variant.as_deref(), Some("danger"));
        let cols = &tree.children()[3];
        assert_eq!(cols.children().len(), 2);
        assert_eq!(cols.children()[0].children()[0].text.as_deref(), Some("left"));
        let table = &tree.children()[4];
        assert_eq!(table.rows.as_ref().unwrap().len(), 2);
        assert_eq!(table.selected, Some(1));
        // unknown component types parse fine, unknown attrs are ignored
        assert_eq!(tree.children()[5].t, "future_widget");
    }

    #[test]
    fn parses_image_map_node() {
        // Shape produced by Builder#image_map in lich-5 dsl.rb
        let raw = r#"{ "t": "image_map", "cid": "image_map:0",
            "src": "/files/cbcal/greyscale/hinterwilds/angargeist.png",
            "scale": 2.0, "scroll_to": "head",
            "popup": "calibrate_creaturebar/settings", "popup_size": [420, 560],
            "markers": [
              { "id": "head", "x1": 40, "y1": 8, "x2": 54, "y2": 22, "kind": "current", "label": "head" },
              { "id": "chest", "x1": 38, "y1": 40, "x2": 52, "y2": 54, "kind": "marker" }
            ] }"#;
        let node: WebUiNode = serde_json::from_str(raw).unwrap();
        assert_eq!(node.t, "image_map");
        assert_eq!(node.scale, Some(2.0));
        assert_eq!(node.scroll_to.as_deref(), Some("head"));
        assert_eq!(node.popup_size, Some([420.0, 560.0]));
        let markers = node.markers.as_ref().unwrap();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].id, "head");
        assert_eq!(markers[0].kind.as_deref(), Some("current"));
        assert_eq!(markers[0].x2, 54.0);
        assert_eq!(markers[1].label, None);
    }

    #[test]
    fn parses_grid_node_sample() {
        // Spec sample from lich5-docker/docs/webui-grid-node.md: 3-col,
        // 6-cell matrix of unlabeled checkboxes under a text header cell.
        let raw = include_str!("../../tests/data/webui-grid-node-sample.json");
        let node: WebUiNode = serde_json::from_str(raw).unwrap();
        assert_eq!(node.t, "grid");
        assert_eq!(node.cols, Some(3));
        assert_eq!(node.children().len(), 6);
        assert_eq!(node.children()[0].t, "cell");
        assert_eq!(node.children()[0].children()[0].t, "text");
        let checkbox = &node.children()[2].children()[0];
        assert_eq!(checkbox.t, "checkbox");
        assert_eq!(checkbox.checked, Some(true));
        assert_eq!(checkbox.label.as_deref(), Some(""));
    }

    #[test]
    fn unknown_envelope_type_is_tolerated() {
        let msg: WebUiServerMessage =
            serde_json::from_str(r#"{"type":"shiny_new_thing","stuff":1}"#).unwrap();
        assert_eq!(msg, WebUiServerMessage::Unknown);
    }

    #[test]
    fn serializes_client_messages() {
        let sub = WebUiClientMessage::Subscribe { page: "creaturebar/main".into() };
        assert_eq!(
            serde_json::to_string(&sub).unwrap(),
            r#"{"type":"subscribe","page":"creaturebar/main"}"#
        );
        let event = WebUiClientMessage::Event {
            page: "demo/demo".into(),
            cid: "button:2".into(),
            value: serde_json::Value::Null,
        };
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"type":"event","page":"demo/demo","cid":"button:2","value":null}"#
        );
        let row = WebUiClientMessage::Event {
            page: "demo/demo".into(),
            cid: "table:4".into(),
            value: serde_json::json!(1),
        };
        assert!(serde_json::to_string(&row).unwrap().contains(r#""value":1"#));
    }
}
