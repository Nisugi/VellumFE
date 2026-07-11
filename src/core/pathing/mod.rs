//! Native pathfinding over the mapdb wayto graph — the Lich `Map` pathing
//! API (map_base.rb) without a Ruby session. First piece of the go2 port
//! (docs/go2-port-plan.md).
//!
//! v1 graph rules (deliberately conservative; see `edge_cost`):
//! - A wayto edge is only routable with a numeric `timeto` cost — exactly
//!   Lich's dijkstra, which skips edges whose weight is nil.
//! - StringProc wayto commands (`";e …"`) are excluded until the transpiler
//!   lands: a path we can't *walk* is worse than no path.
//! - StringProc timeto gates (portmasters, day passes, urchins) evaluate as
//!   "off" — the edge is excluded, matching those settings' v1 defaults.
//! - Urchin-hideout teleport nodes never route.

pub mod dijkstra;

pub use dijkstra::{
    dijkstra, estimate_time, find_nearest, find_nearest_by_tag, path_to, PathTarget,
};
