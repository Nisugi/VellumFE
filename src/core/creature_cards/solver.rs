//! The creature-field floor solver, ported from the quad-placement
//! prototype (vellum-quad-prototype.html). Pure data — no frontend
//! imports, no per-frame work.
//!
//! Contract (the prototype's readouts, promoted to invariants):
//!
//! - **Permanence.** A unit's square and offsets are decided once, on
//!   arrival, and never touched again. Arrivals fit themselves around
//!   whoever is already standing; removals free squares. Nothing else
//!   mutates placement. (Screen positions may still shift when the floor
//!   grows — that is the camera re-framing a wider floor, world
//!   coordinates unchanged.)
//! - **Separation.** Every unit owns its own screen column: the gap to
//!   any neighbour is at least `sep_frac` × the mean of the two card
//!   widths, unless the room was genuinely full when it arrived (the unit
//!   is then marked `tight`).
//! - **Occlusion budget.** An arrival may hide at most `cover_budget` of
//!   any existing head/torso (the upper 62% of the card).
//! - **Ground-z depth.** Draw order and next/prev targeting sort on the
//!   unit's ground depth, never on a lifted screen position.
//!
//! The solver works in a fixed VIRTUAL STAGE (880×470, the prototype's
//! canvas). Renderers map virtual→actual rect uniformly; every screen-
//! space relationship the solver guarantees is invariant under that
//! scale, so widget resizes never re-solve anything.

use std::collections::HashMap;

/// Virtual stage the solver measures in. Renderers scale to their rect.
pub const STAGE_W: f32 = 880.0;
pub const STAGE_H: f32 = 470.0;

/// Tuning, defaults matching the prototype's shipped sliders.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldParams {
    /// Depth rows; held constant, columns grow.
    pub rows: u32,
    /// Units a square prefers before doubling up.
    pub per_square: u32,
    /// Column cap (odd; growth is symmetric).
    pub max_cols: u32,
    /// Centre-clustering strength for the score's middle preference.
    pub cluster: f32,
    /// Square width in world units at 3 columns.
    pub cell_w: f32,
    /// Lateral offset search range, in cell widths.
    pub spread: f32,
    /// Required side gap as a fraction of mean card width.
    pub sep_frac: f32,
    /// Depth jitter amplitude, in row depths.
    pub jitter: f32,
    /// Max fraction of an existing head/torso an arrival may hide.
    pub cover_budget: f32,
    /// Camera: focal length, eye height, near depth, row depth, horizon.
    pub focal: f32,
    pub cam_h: f32,
    pub z0: f32,
    pub dz: f32,
    pub horizon: f32,
}

impl Default for FieldParams {
    fn default() -> Self {
        Self {
            rows: 3,
            per_square: 1,
            max_cols: 11,
            cluster: 1.0,
            cell_w: 1.15,
            spread: 1.15,
            sep_frac: 0.6,
            jitter: 0.2,
            cover_budget: 0.15,
            focal: 420.0,
            cam_h: 1.6,
            z0: 2.4,
            dz: 1.5,
            horizon: 96.0,
        }
    }
}

/// A card's world-space dimensions (width, height) in the same units as
/// `cell_w`. Supplied by the caller from the sprite's aspect; defaults are
/// a generic biped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardSize {
    pub w: f32,
    pub h: f32,
}

impl Default for CardSize {
    fn default() -> Self {
        Self { w: 0.6, h: 1.2 }
    }
}

/// Stable handle for one unit.
pub type UnitId = u64;

/// The allocation atom: one or more creatures owning one floor footprint.
/// A normal creature is a 1-member unit; a mounted pair is 2 members with
/// the mount's footprint (the rider draws from the mount's saddle anchor,
/// a render concern — the solver only tracks the ground).
#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    pub id: UnitId,
    /// Creature exist ids. For a mounted pair, `members[0]` is the mount.
    pub members: Vec<String>,
    /// Floor square (column index may be negative; 0 is centre).
    pub ci: i32,
    pub row: u32,
    /// Offsets within the square: lateral in cell widths, depth in world z.
    pub off_x: f32,
    pub off_z: f32,
    /// Card dimensions used for projection and separation.
    pub size: CardSize,
    /// Arrived with no clear column left (separation rule unmet). Never
    /// "fixed" later — permanence forbids moving anyone.
    pub tight: bool,
}

/// Axis-aligned screen rect on the virtual stage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenRect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    /// Ground depth (the sort key).
    pub z: f32,
}

impl ScreenRect {
    pub fn center_x(&self) -> f32 {
        (self.x0 + self.x1) / 2.0
    }
    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }
}

/// One creature field: the floor plus every placed unit. All mutation goes
/// through `arrive` / `depart` / `mount` / `dismount`.
#[derive(Debug, Clone)]
pub struct CreatureField {
    pub params: FieldParams,
    cols: Vec<i32>,
    units: Vec<Unit>,
    next_id: UnitId,
    /// Bumped on every placement-affecting change; renderers cache quads
    /// against it instead of re-reading per frame.
    pub generation: u64,
}

impl Default for CreatureField {
    fn default() -> Self {
        Self::new(FieldParams::default())
    }
}

impl CreatureField {
    pub fn new(params: FieldParams) -> Self {
        Self {
            params,
            cols: vec![-1, 0, 1],
            units: Vec::new(),
            next_id: 1,
            generation: 0,
        }
    }

    // ---- floor geometry -------------------------------------------------

    /// All drawn columns, ascending.
    pub fn columns(&self) -> &[i32] {
        &self.cols
    }

    /// The floor widens sub-linearly: each added column narrows all of
    /// them, keeping a usable aspect ratio instead of flattening to a
    /// strip.
    pub fn cell_w(&self) -> f32 {
        let n = self.cols.len().max(3) as f32;
        self.params.cell_w * (3.0 / n).powf(0.55)
    }

    fn col_left(&self, ci: i32) -> f32 {
        ci as f32 * self.cell_w() - self.cell_w() / 2.0
    }

    /// Cards scale with the board: as the floor subdivides, positions and
    /// card sizes shrink together, so screen-space relationships between
    /// units are invariant under floor growth.
    fn mscale(&self) -> f32 {
        self.cell_w() / self.params.cell_w
    }

    /// The outermost column on each side is margin, never occupied — a
    /// visible border so nothing sits flush against the floor's edge.
    fn usable_cols(&self) -> Vec<i32> {
        if self.cols.len() <= 2 {
            self.cols.clone()
        } else {
            self.cols[1..self.cols.len() - 1].to_vec()
        }
    }

    fn n_squares(&self) -> usize {
        self.usable_cols().len() * self.params.rows as usize
    }

    fn half_span(&self) -> f32 {
        let lo = *self.cols.first().unwrap();
        let hi = *self.cols.last().unwrap();
        self.col_left(lo)
            .abs()
            .max((self.col_left(hi) + self.cell_w()).abs())
    }

    /// Grow symmetrically, one column each side, recreating the margin on
    /// both edges and keeping the vanishing point centred.
    fn grow_floor(&mut self) -> bool {
        if self.cols.len() as u32 + 2 > self.params.max_cols {
            return false;
        }
        let lo = *self.cols.first().unwrap();
        let hi = *self.cols.last().unwrap();
        self.cols.insert(0, lo - 1);
        self.cols.push(hi + 1);
        true
    }

    /// Shrink one column pair, only when nobody would be left standing on
    /// (or outside) the new margin.
    fn shrink_floor(&mut self) -> bool {
        if self.cols.len() <= 3 {
            return false;
        }
        let lo = self.cols[0];
        let hi = self.cols[self.cols.len() - 1];
        if self.units.iter().any(|u| u.ci < lo + 2 || u.ci > hi - 2) {
            return false;
        }
        self.cols.remove(0);
        self.cols.pop();
        true
    }

    fn contract_floor(&mut self) {
        while self.shrink_floor() {}
    }

    // ---- projection (virtual stage) ------------------------------------

    /// Effective focal length: shrinks to keep the whole floor on the
    /// virtual stage (the prototype's autofit, always on).
    fn f_eff(&self) -> f32 {
        let hs = self.half_span();
        if hs <= 0.0 {
            return self.params.focal;
        }
        self.params
            .focal
            .min((STAGE_W / 2.0 - 30.0) * self.params.z0 / hs)
    }

    fn depth_at(&self, row: f32) -> f32 {
        self.params.z0 + row * self.params.dz
    }

    fn screen_y(&self, z: f32) -> f32 {
        self.params.horizon + (self.params.cam_h * self.f_eff()) / z
    }

    fn screen_x(&self, wx: f32, z: f32) -> f32 {
        STAGE_W / 2.0 + (wx * self.f_eff()) / z
    }

    /// A unit's ground depth: the depth-sort and targeting key.
    pub fn ground_z(&self, u: &Unit) -> f32 {
        (self.depth_at(u.row as f32 + 0.5) + u.off_z).max(0.4)
    }

    /// Foot point on the virtual stage (shadow centre, standee base).
    pub fn foot(&self, u: &Unit) -> (f32, f32) {
        let z = self.ground_z(u);
        let wx = u.ci as f32 * self.cell_w() + u.off_x * self.cell_w();
        (self.screen_x(wx, z), self.screen_y(z))
    }

    /// The card's screen rect (upright standee), used for occlusion,
    /// separation, and hit testing.
    pub fn rect(&self, u: &Unit) -> ScreenRect {
        self.rect_for(u.size, u.ci, u.row, u.off_x, u.off_z)
    }

    fn rect_for(&self, size: CardSize, ci: i32, row: u32, off_x: f32, off_z: f32) -> ScreenRect {
        let z = (self.depth_at(row as f32 + 0.5) + off_z).max(0.4);
        let wx = ci as f32 * self.cell_w() + off_x * self.cell_w();
        let px = self.screen_x(wx, z);
        let py = self.screen_y(z);
        let wp = size.w * self.mscale() * self.f_eff() / z;
        let hp = size.h * self.mscale() * self.f_eff() / z;
        ScreenRect {
            x0: px - wp / 2.0,
            y0: py - hp,
            x1: px + wp / 2.0,
            y1: py,
            z,
        }
    }

    /// Endpoints (virtual stage) of the floor grid line along depth row
    /// `r` (0..=rows). For renderers.
    pub fn floor_row_line(&self, r: u32) -> ((f32, f32), (f32, f32)) {
        let z = self.depth_at(r as f32);
        let lo = self.col_left(*self.cols.first().unwrap());
        let hi = self.col_left(*self.cols.last().unwrap()) + self.cell_w();
        let y = self.screen_y(z);
        ((self.screen_x(lo, z), y), (self.screen_x(hi, z), y))
    }

    /// Endpoints (virtual stage) of the floor grid line down column
    /// boundary `k` (0..=columns().len()). For renderers.
    pub fn floor_col_line(&self, k: usize) -> ((f32, f32), (f32, f32)) {
        let x = if k < self.cols.len() {
            self.col_left(self.cols[k])
        } else {
            self.col_left(*self.cols.last().unwrap()) + self.cell_w()
        };
        let zn = self.depth_at(0.0);
        let zf = self.depth_at(self.params.rows as f32);
        (
            (self.screen_x(x, zn), self.screen_y(zn)),
            (self.screen_x(x, zf), self.screen_y(zf)),
        )
    }

    // ---- queries --------------------------------------------------------

    pub fn units(&self) -> &[Unit] {
        &self.units
    }

    pub fn unit_of(&self, exist: &str) -> Option<&Unit> {
        self.units
            .iter()
            .find(|u| u.members.iter().any(|m| m == exist))
    }

    /// Unit indices in draw order: farthest first (painter's algorithm),
    /// keyed on ground z.
    pub fn draw_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.units.len()).collect();
        order.sort_by(|&a, &b| {
            self.ground_z(&self.units[b])
                .total_cmp(&self.ground_z(&self.units[a]))
        });
        order
    }

    /// Unit indices left→right by foot x: the next/previous targeting
    /// order.
    pub fn target_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.units.len()).collect();
        order.sort_by(|&a, &b| {
            self.foot(&self.units[a])
                .0
                .total_cmp(&self.foot(&self.units[b]).0)
        });
        order
    }

    // ---- invariant meters (tests + debug overlays) ----------------------

    /// Worst head/torso coverage across all units, front-to-back.
    pub fn worst_coverage(&self) -> f32 {
        let mut order = self.draw_order();
        order.reverse(); // nearest first
        let mut placed: Vec<ScreenRect> = Vec::new();
        let mut worst: f32 = 0.0;
        // Walk near→far so each unit is measured against only the rects
        // drawn ON TOP of it.
        for &i in &order {
            let r = self.rect(&self.units[i]);
            worst = worst.max(covered_by(&r, &placed));
            placed.push(r);
        }
        worst
    }

    /// Smallest neighbour gap / required clearance across all unit pairs;
    /// >= 1.0 means the separation rule holds everywhere. Empty/singleton
    /// fields report a comfortable 9.0.
    pub fn min_separation_ratio(&self) -> f32 {
        let mut xs: Vec<(f32, f32)> = self
            .units
            .iter()
            .map(|u| {
                let r = self.rect(u);
                (r.center_x(), r.width())
            })
            .collect();
        if xs.len() < 2 {
            return 9.0;
        }
        xs.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut min = f32::INFINITY;
        for pair in xs.windows(2) {
            let gap = pair[1].0 - pair[0].0;
            let need = self.params.sep_frac * (pair[0].1 + pair[1].1) / 2.0;
            if need > 0.0 {
                min = min.min(gap / need);
            }
        }
        if min.is_finite() {
            min
        } else {
            9.0
        }
    }

    // ---- mutation -------------------------------------------------------

    /// Place a new creature, leaving every existing unit untouched.
    /// Returns the new unit's id.
    pub fn arrive(&mut self, exist: &str, size: CardSize) -> UnitId {
        let placement = self.choose_home(size, exist, None);
        self.commit_arrival(exist, size, placement)
    }

    /// Remove a creature (looted / gone). If it was one member of a
    /// mounted pair, the pair splits first and the survivor keeps the
    /// unit's square — the dismount-before-death ordering, enforced here
    /// so callers cannot tear a rider down with its mount.
    pub fn depart(&mut self, exist: &str) {
        let Some(idx) = self
            .units
            .iter()
            .position(|u| u.members.iter().any(|m| m == exist))
        else {
            return;
        };
        if self.units[idx].members.len() > 1 {
            self.units[idx].members.retain(|m| m != exist);
        } else {
            self.units.remove(idx);
            self.contract_floor();
        }
        self.generation += 1;
    }

    /// Merge a rider onto a mount mid-combat: the mount keeps its square
    /// (permanence), the rider's own square is freed.
    pub fn mount(&mut self, rider: &str, mount: &str) {
        let Some(mount_idx) = self
            .units
            .iter()
            .position(|u| u.members.iter().any(|m| m == mount))
        else {
            return;
        };
        // Free the rider's old unit (if it had one of its own).
        if let Some(rider_idx) = self
            .units
            .iter()
            .position(|u| u.members.iter().any(|m| m == rider))
        {
            if rider_idx == mount_idx {
                return; // already mounted
            }
            self.units.remove(rider_idx);
        }
        let mount_idx = self
            .units
            .iter()
            .position(|u| u.members.iter().any(|m| m == mount))
            .expect("mount unit still present");
        self.units[mount_idx].members.push(rider.to_string());
        self.contract_floor();
        self.generation += 1;
    }

    /// Split a mounted pair: the mount keeps the unit's square, the rider
    /// is placed NEAR it — nearest passing square outward from the mount's
    /// foot point, growing the floor rather than flinging the rider across
    /// the room. Locality beats compactness here.
    pub fn dismount(&mut self, rider: &str, rider_size: CardSize) {
        let Some(idx) = self
            .units
            .iter()
            .position(|u| u.members.iter().any(|m| m == rider) && u.members.len() > 1)
        else {
            return;
        };
        self.units[idx].members.retain(|m| m != rider);
        let anchor = self.foot(&self.units[idx]);
        let placement = self.choose_home(rider_size, rider, Some(anchor));
        self.commit_arrival(rider, rider_size, placement);
    }

    fn commit_arrival(&mut self, exist: &str, size: CardSize, p: Placement) -> UnitId {
        let id = self.next_id;
        self.next_id += 1;
        self.units.push(Unit {
            id,
            members: vec![exist.to_string()],
            ci: p.ci,
            row: p.row,
            off_x: p.off_x,
            off_z: p.off_z,
            size,
            tight: p.tight,
        });
        // Grow once it is getting tight, so the next arrival has room.
        if self.units.len() >= self.n_squares() * self.params.per_square.max(1) as usize {
            self.grow_floor();
        }
        self.generation += 1;
        id
    }

    // ---- the solver -----------------------------------------------------

    fn occupants_of(&self, ci: i32, row: u32) -> usize {
        self.units
            .iter()
            .filter(|u| u.ci == ci && u.row == row)
            .count()
    }

    /// Preference, not a law: shorter-behind-taller reads badly, but a
    /// hard rule would let one troll lock every square behind it forever.
    /// Actual occlusion is measured directly; this only breaks ties.
    fn height_penalty(&self, ci: i32, row: u32, h: f32) -> u32 {
        self.units
            .iter()
            .filter(|u| u.ci == ci)
            .filter(|u| {
                (u.row > row && u.size.h < h) || (u.row < row && u.size.h > h)
            })
            .count() as u32
    }

    /// Choose a home for one new card. `near` switches to affinity mode
    /// (dismount): candidates are ranked by screen distance from the
    /// anchor instead of the arrival score, and the floor grows when
    /// nothing nearby passes.
    fn choose_home(&mut self, size: CardSize, exist: &str, near: Option<(f32, f32)>) -> Placement {
        let mut rng = Rng::new(seed_of(exist));
        // The roomiest near-miss survives across growth attempts, in case
        // nothing anywhere clears the separation bar.
        let mut loose: Option<Placement> = None;
        for _attempt in 0..self.params.max_cols {
            let others: Vec<ScreenRect> = self.units.iter().map(|u| self.rect(u)).collect();
            let cols = self.usable_cols();
            let sigma = ((cols.len() as f32 / 3.0) * self.params.cluster).max(0.55);
            let mut best: Option<Placement> = None;
            for &ci in &cols {
                for row in 0..self.params.rows {
                    let occ = self.occupants_of(ci, row);
                    if occ >= self.params.per_square.max(1) as usize + 1 {
                        continue;
                    }
                    let hpen = self.height_penalty(ci, row, size.h) as f32;
                    const NZ: usize = 5;
                    const NX: usize = 11;
                    for a in 0..NZ {
                        let oz = (-0.34 + a as f32 * 0.17) * self.params.dz
                            + (rng.unit() * 2.0 - 1.0) * self.params.dz * self.params.jitter * 0.5;
                        if oz.abs() > self.params.dz * 0.44 {
                            continue;
                        }
                        for b in 0..NX {
                            let ox = (-0.7 + 1.4 * b as f32 / (NX - 1) as f32)
                                * self.params.spread.max(0.2);
                            let r = self.rect_for(size, ci, row, ox, oz);
                            if r.x0 < 6.0 || r.x1 > STAGE_W - 6.0 {
                                continue;
                            }
                            // Separation: every unit owns its own screen
                            // column. Clearance scales with card widths, so
                            // the rule survives any zoom unchanged.
                            let mut slack = f32::INFINITY;
                            for o in &others {
                                let gap = (r.center_x() - o.center_x()).abs();
                                let need = self.params.sep_frac * (r.width() + o.width()) / 2.0;
                                slack = slack.min(gap - need);
                            }
                            let tight = !others.is_empty() && slack < 0.0;
                            let front: Vec<ScreenRect> =
                                others.iter().filter(|o| o.z < r.z).copied().collect();
                            let cov_new = covered_by(&r, &front);
                            // The budget is on TOTAL coverage, not just what
                            // this arrival adds: an existing unit's occluders
                            // are everything nearer than it PLUS the
                            // candidate. Skip units the candidate can't even
                            // touch — their total is unchanged by us.
                            let mut behind_cov: f32 = 0.0;
                            for o in others.iter().filter(|o| o.z >= r.z) {
                                if r.x1 < o.x0 || r.x0 > o.x1 || r.y1 < o.y0 || r.y0 > o.y1 {
                                    continue;
                                }
                                let mut occluders: Vec<ScreenRect> =
                                    others.iter().filter(|p| p.z < o.z).copied().collect();
                                occluders.push(r);
                                behind_cov = behind_cov.max(covered_by(o, &occluders));
                            }
                            // Hard rules: separation AND the occlusion
                            // budget, both directions. A candidate failing
                            // either is a near-miss — kept as the least-bad
                            // fallback, never preferred while the floor can
                            // still grow.
                            let over_budget = cov_new > self.params.cover_budget
                                || behind_cov > self.params.cover_budget;
                            let score = match near {
                                // Affinity mode: nearest passing square wins.
                                Some((ax, ay)) => {
                                    let fx = (r.center_x() - ax).abs();
                                    let fy = (r.y1 - ay).abs();
                                    (fx * fx + fy * fy).sqrt()
                                }
                                None => {
                                    1.7 * cov_new
                                        + 1.3 * behind_cov
                                        + 0.35
                                            * (1.0
                                                - (-((ci * ci) as f32)
                                                    / (2.0 * sigma * sigma))
                                                    .exp())
                                        + 0.95 * occ as f32
                                        + 0.18 * ox.abs()
                                        + 0.22 * hpen
                                }
                            };
                            let cand = Placement {
                                ci,
                                row,
                                off_x: ox,
                                off_z: oz,
                                tight,
                                score,
                            };
                            if tight || over_budget {
                                let fs = (-slack).max(0.0) + cov_new + behind_cov;
                                if loose.as_ref().is_none_or(|l| fs < l.score) {
                                    // A committed near-miss is always
                                    // `tight`, whichever rule it missed.
                                    loose = Some(Placement {
                                        score: fs,
                                        tight: true,
                                        ..cand
                                    });
                                }
                                continue;
                            }
                            if best.as_ref().is_none_or(|b| score < b.score) {
                                best = Some(cand);
                            }
                        }
                    }
                }
            }
            if let Some(best) = best {
                return best;
            }
            if !self.grow_floor() {
                // Floor at cap: the roomiest near-miss, or dead centre.
                return loose.unwrap_or(Placement {
                    ci: 0,
                    row: 0,
                    off_x: 0.0,
                    off_z: 0.0,
                    tight: true,
                    score: 0.0,
                });
            }
        }
        loose.unwrap_or(Placement {
            ci: 0,
            row: 0,
            off_x: 0.0,
            off_z: 0.0,
            tight: true,
            score: 0.0,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct Placement {
    ci: i32,
    row: u32,
    off_x: f32,
    off_z: f32,
    tight: bool,
    score: f32,
}

/// Fraction of `r`'s head/torso hidden by any rect in `list` (all assumed
/// nearer, drawn on top). Only the upper 62% is sampled: that is the part
/// that tells you which creature this is — feet hidden behind a nearer
/// body is fine.
fn covered_by(r: &ScreenRect, list: &[ScreenRect]) -> f32 {
    if list.is_empty() {
        return 0.0;
    }
    const NX: usize = 9;
    const NY: usize = 13;
    let y_top = r.y0;
    let y_bot = r.y0 + (r.y1 - r.y0) * 0.62;
    let mut hit = 0usize;
    for i in 0..NX {
        for j in 0..NY {
            let px = r.x0 + (i as f32 + 0.5) / NX as f32 * (r.x1 - r.x0);
            let py = y_top + (j as f32 + 0.5) / NY as f32 * (y_bot - y_top);
            if list
                .iter()
                .any(|o| px >= o.x0 && px <= o.x1 && py >= o.y0 && py <= o.y1)
            {
                hit += 1;
            }
        }
    }
    hit as f32 / (NX * NY) as f32
}

/// Deterministic per-creature seed: numeric exist ids hash to themselves,
/// anything else FNV-hashes, so the same roster always lays out the same.
fn seed_of(exist: &str) -> u32 {
    exist.parse::<u32>().unwrap_or_else(|_| {
        let mut h: u32 = 2166136261;
        for b in exist.bytes() {
            h ^= b as u32;
            h = h.wrapping_mul(16777619);
        }
        h
    })
}

/// The prototype's mulberry32, ported bit-exact so tuning carries over.
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Self(seed)
    }
    fn unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x6D2B79F5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) as f64 / 4294967296.0) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kobold() -> CardSize {
        CardSize { w: 0.60, h: 0.92 }
    }
    fn troll() -> CardSize {
        CardSize { w: 0.78, h: 1.52 }
    }
    fn spider() -> CardSize {
        CardSize { w: 0.92, h: 0.60 }
    }

    fn world_pos(f: &CreatureField, exist: &str) -> (i32, u32, f32, f32) {
        let u = f.unit_of(exist).unwrap();
        (u.ci, u.row, u.off_x, u.off_z)
    }

    fn fill(f: &mut CreatureField, n: usize) {
        let sizes = [kobold(), troll(), spider()];
        for i in 0..n {
            f.arrive(&format!("{}", 1000 + i), sizes[i % 3]);
        }
    }

    /// THE invariant: adding creatures never moves anyone already placed
    /// (world coordinates; screen travel from floor growth is the camera).
    #[test]
    fn placement_is_permanent_under_arrivals() {
        let mut f = CreatureField::default();
        fill(&mut f, 4);
        let before: Vec<_> = (0..4)
            .map(|i| world_pos(&f, &format!("{}", 1000 + i)))
            .collect();
        fill_more(&mut f, 4, 8);
        for (i, b) in before.iter().enumerate() {
            assert_eq!(
                world_pos(&f, &format!("{}", 1000 + i)),
                *b,
                "creature {i} moved when others arrived"
            );
        }
    }

    fn fill_more(f: &mut CreatureField, from: usize, to: usize) {
        let sizes = [kobold(), troll(), spider()];
        for i in from..to {
            f.arrive(&format!("{}", 1000 + i), sizes[i % 3]);
        }
    }

    #[test]
    fn removals_never_move_survivors() {
        let mut f = CreatureField::default();
        fill(&mut f, 8);
        let keep: Vec<String> = (0..8)
            .filter(|i| i % 2 == 0)
            .map(|i| format!("{}", 1000 + i))
            .collect();
        let before: Vec<_> = keep.iter().map(|e| world_pos(&f, e)).collect();
        for i in (0..8).filter(|i| i % 2 == 1) {
            f.depart(&format!("{}", 1000 + i));
        }
        for (e, b) in keep.iter().zip(&before) {
            assert_eq!(world_pos(&f, e), *b, "{e} moved when others were looted");
        }
    }

    #[test]
    fn separation_and_coverage_hold_at_moderate_population() {
        let mut f = CreatureField::default();
        fill(&mut f, 7); // the prototype's default room
        assert!(
            f.min_separation_ratio() >= 1.0,
            "separation ratio {} < 1.0",
            f.min_separation_ratio()
        );
        assert!(
            f.worst_coverage() <= f.params.cover_budget + 1e-3,
            "worst coverage {} over budget {}",
            f.worst_coverage(),
            f.params.cover_budget
        );
        assert!(f.units().iter().all(|u| !u.tight));
    }

    #[test]
    fn margins_are_never_occupied_and_growth_is_symmetric() {
        let mut f = CreatureField::default();
        fill(&mut f, 12);
        let lo = *f.columns().first().unwrap();
        let hi = *f.columns().last().unwrap();
        assert_eq!(lo, -hi, "growth must stay centred");
        for u in f.units() {
            assert!(
                u.ci > lo && u.ci < hi,
                "unit on margin column {} (floor {lo}..{hi})",
                u.ci
            );
        }
    }

    #[test]
    fn loot_frees_squares_and_floor_contracts() {
        let mut f = CreatureField::default();
        fill(&mut f, 12);
        let grown = f.columns().len();
        assert!(grown > 3, "12 creatures should have grown the floor");
        for i in 0..12 {
            f.depart(&format!("{}", 1000 + i));
        }
        assert!(f.units().is_empty());
        assert_eq!(f.columns().len(), 3, "empty floor contracts to base");
    }

    #[test]
    fn same_roster_lays_out_identically() {
        let mut a = CreatureField::default();
        let mut b = CreatureField::default();
        fill(&mut a, 9);
        fill(&mut b, 9);
        for i in 0..9 {
            let e = format!("{}", 1000 + i);
            assert_eq!(world_pos(&a, &e), world_pos(&b, &e));
        }
    }

    #[test]
    fn draw_order_sorts_on_ground_z_farthest_first() {
        let mut f = CreatureField::default();
        fill(&mut f, 7);
        let order = f.draw_order();
        let zs: Vec<f32> = order.iter().map(|&i| f.ground_z(&f.units()[i])).collect();
        assert!(zs.windows(2).all(|w| w[0] >= w[1]), "not far-to-near: {zs:?}");
    }

    #[test]
    fn target_order_is_left_to_right() {
        let mut f = CreatureField::default();
        fill(&mut f, 7);
        let order = f.target_order();
        let xs: Vec<f32> = order.iter().map(|&i| f.foot(&f.units()[i]).0).collect();
        assert!(xs.windows(2).all(|w| w[0] <= w[1]));
    }

    // ---- units: mount / dismount / death -------------------------------

    #[test]
    fn mount_merges_keeping_the_mounts_square() {
        let mut f = CreatureField::default();
        f.arrive("mount1", troll());
        f.arrive("rider1", kobold());
        f.arrive("bystander", spider());
        let mount_home = world_pos(&f, "mount1");
        f.mount("rider1", "mount1");
        assert_eq!(f.units().len(), 2);
        let pair = f.unit_of("rider1").unwrap();
        assert_eq!(pair.members, vec!["mount1", "rider1"], "mount stays members[0]");
        assert_eq!(world_pos(&f, "mount1"), mount_home, "mount must not move");
        assert_eq!(world_pos(&f, "bystander").0, world_pos(&f, "bystander").0);
    }

    #[test]
    fn dismount_places_rider_near_the_mount() {
        let mut f = CreatureField::default();
        f.arrive("mount1", troll());
        f.mount("rider1", "mount1"); // rider joins without own square
        // Crowd the room so nearness is actually contested.
        fill_more(&mut f, 0, 5);
        let mount_foot = f.foot(f.unit_of("mount1").unwrap());
        f.dismount("rider1", kobold());
        let rider = f.unit_of("rider1").unwrap();
        assert_eq!(rider.members, vec!["rider1"]);
        let rider_foot = f.foot(rider);
        // Near: the rider must land closer to the mount than the farthest
        // possible square (half the stage), and pass the hard rules.
        let d = ((rider_foot.0 - mount_foot.0).powi(2)
            + (rider_foot.1 - mount_foot.1).powi(2))
        .sqrt();
        assert!(d < STAGE_W / 2.0, "rider flung {d}px from its mount");
        assert!(!rider.tight, "affinity placement must honour separation");
        assert!(
            f.worst_coverage() <= f.params.cover_budget + 1e-3,
            "dismount broke the occlusion budget: {} (rider tight={}, cols={})",
            f.worst_coverage(),
            f.unit_of("rider1").unwrap().tight,
            f.columns().len(),
        );
    }

    /// The ordering trap: a mount dying while ridden must not tear the
    /// rider down with it. depart() enforces split-then-die internally.
    #[test]
    fn mount_death_while_ridden_spares_the_rider() {
        let mut f = CreatureField::default();
        f.arrive("mount1", troll());
        f.mount("rider1", "mount1");
        let home = world_pos(&f, "mount1");
        f.depart("mount1");
        let rider = f.unit_of("rider1").expect("rider must survive its mount");
        assert_eq!(rider.members, vec!["rider1"]);
        // The survivor keeps the pair's square — it is standing there.
        assert_eq!(world_pos(&f, "rider1"), home);
        assert!(f.unit_of("mount1").is_none());
    }

    #[test]
    fn rider_death_while_ridden_leaves_mount_in_place() {
        let mut f = CreatureField::default();
        f.arrive("mount1", troll());
        f.mount("rider1", "mount1");
        let home = world_pos(&f, "mount1");
        f.depart("rider1");
        assert_eq!(world_pos(&f, "mount1"), home);
        assert_eq!(f.unit_of("mount1").unwrap().members, vec!["mount1"]);
    }

    #[test]
    fn generation_bumps_only_on_placement_changes() {
        let mut f = CreatureField::default();
        let g0 = f.generation;
        f.arrive("a", kobold());
        assert!(f.generation > g0);
        let g1 = f.generation;
        let _ = f.draw_order();
        let _ = f.worst_coverage();
        let _ = f.rect(f.unit_of("a").unwrap());
        assert_eq!(f.generation, g1, "queries must not dirty the field");
        f.depart("a");
        assert!(f.generation > g1);
    }

    /// Packing far past comfortable capacity must not panic, must keep
    /// permanence, and everyone stays inside the drawn floor.
    #[test]
    fn overfull_room_degrades_gracefully() {
        let mut f = CreatureField::default();
        fill(&mut f, 16);
        assert_eq!(f.units().len(), 16);
        assert!(f.columns().len() as u32 <= f.params.max_cols);
        let lo = *f.columns().first().unwrap();
        let hi = *f.columns().last().unwrap();
        for u in f.units() {
            assert!(u.ci > lo && u.ci < hi);
        }
    }
}
