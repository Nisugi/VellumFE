//! The creature-field floor solver, ported from the quad-placement
//! prototype and revised against the field-solver v2 prototype
//! (vellum-field-solver-v2.html). Pure data — no frontend imports, no
//! per-frame work.
//!
//! Contract (the prototype's readouts, promoted to invariants):
//!
//! - **Permanence.** A unit's square and offsets are decided once, on
//!   arrival, and never touched again (Studio `place_at` is the one
//!   explicit override). Arrivals fit themselves around whoever is already
//!   standing; removals free squares. Nothing else mutates placement.
//!   (Screen positions may still shift when the floor grows — that is the
//!   camera re-framing a wider floor, world coordinates unchanged.)
//! - **Separation.** Every unit owns its own screen column: the gap to
//!   any neighbour is at least `sep_frac` × the mean of the two contact
//!   spans (the part of each card that actually rests on the floor — a
//!   tail or a thrown-forward paw overhangs it harmlessly), unless the
//!   room was genuinely full when it arrived (the unit is then marked
//!   `tight`).
//! - **Occlusion cap.** Occlusion is a FEASIBILITY BOUND, not a cost: a
//!   candidate hiding more than `occlusion_cap` of any identity region
//!   (its own or a neighbour's) is rejected outright, and the depth terms
//!   optimise freely inside what remains. Penalising occlusion instead
//!   cannot buy depth — side-by-side cards never overlap, which is exactly
//!   how a cost converges on a straight row.
//! - **Spawn zone.** Feet land inside the ellipse inscribed in the floor's
//!   world extent (shrunk by `zone_inset`); the ellipse supplies the border
//!   the old margin columns used to, so the whole floor gets used.
//! - **Fall envelope.** An arrival reserves room for the pose it is not in
//!   yet: the union of its standing and prone rects must not overlap a
//!   neighbour's envelope past the (relaxable) threshold, because
//!   relocation after the fact is forbidden by permanence.
//! - **Ground-z depth.** Draw order and next/prev targeting sort on the
//!   unit's ground depth, never on a lifted screen position.
//!
//! When the floor hits its column cap the hard rules relax a notch at a
//! time (`relax_steps`) before falling through to the least-bad candidate:
//! loosening in order costs a little overlap; falling through costs a
//! pile-up.
//!
//! The solver works in a fixed VIRTUAL STAGE (880×470, the prototype's
//! canvas). Renderers map virtual→actual rect uniformly; every screen-
//! space relationship the solver guarantees is invariant under that
//! scale, so widget resizes never re-solve anything.

/// Virtual stage the solver measures in. Renderers scale to their rect.
pub const STAGE_W: f32 = 880.0;
pub const STAGE_H: f32 = 470.0;

/// What the separation rule measures a card's width by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeparationBasis {
    /// The contact span — the fraction of the card that rests on the
    /// floor. Extremities may overlap; body mass may not.
    Contact,
    /// The full card box, overhang included.
    Card,
}

/// Placement tunables, defaults matching the v2 prototype's shipped
/// values. Per-field and cloneable; a skin overlays them via
/// [`FieldParams::apply_solver`].
#[derive(Debug, Clone, PartialEq)]
pub struct SolverParams {
    /// Inscribed spawn ellipse on/off ("ellipse" vs "grid" in TOML).
    pub zone_on: bool,
    /// Ellipse shrink from the floor's edge (fraction).
    pub zone_inset: f32,
    /// Radial centre pull inside the ellipse (squared falloff).
    pub centre_pull: f32,
    /// Depth bases sampled per square.
    pub depth_samples: u32,
    /// Depth jitter amplitude, in row depths.
    pub depth_jitter: f32,
    /// Lateral jitter amplitude, in cell widths.
    pub lateral_jitter: f32,
    /// Repulsion from other creatures' world depth.
    pub depth_spread: f32,
    /// Repulsion from other creatures' foot SCREEN y — the term that
    /// actually kills the visual row (the eye reads screen y, not z).
    pub row_band_push: f32,
    /// Row-band kernel width in stage pixels.
    pub row_band_px: f32,
    /// Max identity-region coverage a candidate may cause, either way.
    pub occlusion_cap: f32,
    /// Soft score noise, shuffling which acceptable home is taken.
    pub variation: f32,
    /// First arrival into an empty field goes dead centre, near row.
    pub seed_front: bool,
    /// Fall-envelope overlap cost weight.
    pub fall_reserve: f32,
    /// Whether the worst envelope overlap is a hard (relaxable) bound.
    pub fall_reserve_hard: bool,
    /// Width basis for the separation rule.
    pub separation_basis: SeparationBasis,
    /// Fisher-Yates the candidate list so ties don't bias to a=0.
    pub shuffle_ties: bool,
    /// Constraint-loosening notches once floor growth is exhausted.
    pub relax_steps: u32,
}

impl Default for SolverParams {
    fn default() -> Self {
        Self {
            zone_on: true,
            zone_inset: 0.10,
            centre_pull: 0.45,
            depth_samples: 9,
            depth_jitter: 0.22,
            lateral_jitter: 0.12,
            depth_spread: 0.70,
            row_band_push: 1.60,
            row_band_px: 28.0,
            occlusion_cap: 0.18,
            variation: 0.35,
            seed_front: true,
            fall_reserve: 0.9,
            fall_reserve_hard: true,
            separation_basis: SeparationBasis::Contact,
            shuffle_ties: true,
            relax_steps: 4,
        }
    }
}

/// Tuning, defaults matching the prototype's shipped sliders.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldParams {
    /// Depth rows; held constant, columns grow.
    pub rows: u32,
    /// Units a square prefers before doubling up.
    pub per_square: u32,
    /// Column cap (odd; growth is symmetric).
    pub max_cols: u32,
    /// Square width in world units at 3 columns.
    pub cell_w: f32,
    /// Lateral offset search range, in cell widths.
    pub spread: f32,
    /// Required side gap as a fraction of the mean separation width.
    pub sep_frac: f32,
    /// Camera: focal length, eye height, near depth, row depth, horizon.
    pub focal: f32,
    pub cam_h: f32,
    pub z0: f32,
    pub dz: f32,
    pub horizon: f32,
    /// Placement tunables (the v2 solver knobs).
    pub solver: SolverParams,
}

impl Default for FieldParams {
    fn default() -> Self {
        Self {
            rows: 3,
            per_square: 1,
            max_cols: 11,
            cell_w: 1.15,
            spread: 1.15,
            sep_frac: 0.6,
            focal: 420.0,
            cam_h: 1.6,
            z0: 2.4,
            dz: 1.5,
            horizon: 96.0,
            solver: SolverParams::default(),
        }
    }
}

/// A card's world-space dimensions (width, height) in the same units as
/// `cell_w`, plus the contact span: the fraction of the width that
/// actually rests on the floor (measured ~65% on the standing dummy and
/// coyote — tails, daggers and thrown-forward paws overhang the rest).
/// Supplied by the caller from the sprite's aspect; defaults are a
/// generic biped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardSize {
    pub w: f32,
    pub h: f32,
    pub span: f32,
}

impl CardSize {
    /// A card with the default contact span.
    pub fn new(w: f32, h: f32) -> Self {
        Self { w, h, span: 0.65 }
    }
}

impl Default for CardSize {
    fn default() -> Self {
        Self::new(0.6, 1.2)
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
    /// Card dimensions for the CURRENT pose, used for projection and
    /// separation. `resize` swaps this on pose change.
    pub size: CardSize,
    /// Both pose boxes, fixed at arrival: the fall envelope every later
    /// arrival reserves against is their union.
    pub standing: CardSize,
    pub prone: CardSize,
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

/// AUTHORED exclusion metadata from a scene prop: no creature is ever
/// PLACED behind it (deeper than `z`, card centre inside the projected
/// span) — in front and beside stay open, and permanence still holds
/// (placed units are never relocated, even when a scene swap drops an
/// obstacle on top of them).
///
/// The stored extents are WORLD units, not screen pixels: the screen span
/// depends on the effective projection (`mscale`/`f_eff`, both functions
/// of the column count), so the solver projects the span fresh via
/// [`CreatureField::obstacle_span`] before every placement decision —
/// including mid-search floor growth. A cached pixel span would go stale
/// the moment the floor grew or contracted (105px vs 38.89px for a
/// unit-height prop at depth 4 with 3 vs 11 columns).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Obstacle {
    /// Stage-space screen x of the prop's ground/foot point (props author
    /// their lateral position directly on the stage; the camera never
    /// re-aims at them — see `project_ground`).
    pub x: f32,
    /// The prop's world depth: only candidates deeper than this block.
    pub z: f32,
    /// Exclusion extent left/right of the foot point, in world units at
    /// depth `z` (either may be negative when the calibrated span sits
    /// entirely to one side of the foot).
    pub left_w: f32,
    pub right_w: f32,
}

/// One creature field: the floor plus every placed unit. All mutation goes
/// through `arrive` / `depart` / `mount` / `dismount`.
#[derive(Debug, Clone)]
pub struct CreatureField {
    pub params: FieldParams,
    cols: Vec<i32>,
    units: Vec<Unit>,
    next_id: UnitId,
    /// Scene-prop exclusion spans; arrivals only (see [`Obstacle`]).
    obstacles: Vec<Obstacle>,
    /// Bumped on every placement-affecting change; renderers cache quads
    /// against it instead of re-reading per frame.
    pub generation: u64,
}

impl Default for CreatureField {
    fn default() -> Self {
        Self::new(FieldParams::default())
    }
}

/// The one set of camera bounds every consumer shares: scene loading
/// (`apply_camera`), Studio's live drag controls, and pre-save validation
/// (`sanitize_camera`). An editor previewing a value outside these ranges
/// would save something that reloads differently — so nothing may edit,
/// preview, or serialize outside them.
pub mod camera_limits {
    /// (lo, hi) inclusive bounds per camera key.
    pub const FOCAL: (f32, f32) = (60.0, 4000.0);
    pub const EYE_HEIGHT: (f32, f32) = (0.1, 20.0);
    pub const NEAR_DEPTH: (f32, f32) = (0.1, 50.0);
    pub const ROW_DEPTH: (f32, f32) = (0.05, 20.0);
    pub const HORIZON: (f32, f32) = (-500.0, 2000.0);
    pub const CELL_WIDTH: (f32, f32) = (0.1, 10.0);
}

impl FieldParams {
    /// Force the camera into the shared [`camera_limits`]: nonfinite
    /// values fall back to `fallback`'s field (or the built-in default if
    /// that is nonfinite too), everything clamps into range. Returns true
    /// when anything changed. Editors call this after every edit — the
    /// effective value shows immediately — and before serializing, so an
    /// invalid transient never reaches the renderer or a scene file.
    pub fn sanitize_camera(&mut self, fallback: &FieldParams) -> bool {
        use camera_limits::*;
        fn fix(slot: &mut f32, fallback: f32, default: f32, (lo, hi): (f32, f32)) -> bool {
            let before = *slot;
            if !slot.is_finite() {
                *slot = if fallback.is_finite() { fallback } else { default };
            }
            *slot = slot.clamp(lo, hi);
            *slot != before
        }
        let d = FieldParams::default();
        let mut changed = false;
        changed |= fix(&mut self.focal, fallback.focal, d.focal, FOCAL);
        changed |= fix(&mut self.cam_h, fallback.cam_h, d.cam_h, EYE_HEIGHT);
        changed |= fix(&mut self.z0, fallback.z0, d.z0, NEAR_DEPTH);
        changed |= fix(&mut self.dz, fallback.dz, d.dz, ROW_DEPTH);
        changed |= fix(&mut self.horizon, fallback.horizon, d.horizon, HORIZON);
        changed |= fix(&mut self.cell_w, fallback.cell_w, d.cell_w, CELL_WIDTH);
        changed
    }

    /// Overlay a skin's `[creature_field.camera]` onto these params. Unset
    /// keys keep their current value; out-of-range values clamp to the
    /// nearest sane bound and log — a bad focal degrades the camera, it
    /// never drops the widget.
    pub fn apply_camera(&mut self, cam: &crate::config::skins::CreatureFieldCamera) {
        /// Clamp with a warning naming the offending key.
        fn take(slot: &mut f32, value: Option<f32>, key: &str, lo: f32, hi: f32) {
            let Some(v) = value else { return };
            if !v.is_finite() {
                tracing::warn!("[creature_field.camera] {key} is not a number; keeping {slot}");
                return;
            }
            let c = v.clamp(lo, hi);
            if c != v {
                tracing::warn!(
                    "[creature_field.camera] {key} = {v} out of range {lo}..={hi}; using {c}"
                );
            }
            *slot = c;
        }
        use camera_limits::*;
        take(&mut self.focal, cam.focal, "focal", FOCAL.0, FOCAL.1);
        take(
            &mut self.cam_h,
            cam.eye_height,
            "eye_height",
            EYE_HEIGHT.0,
            EYE_HEIGHT.1,
        );
        take(
            &mut self.z0,
            cam.near_depth,
            "near_depth",
            NEAR_DEPTH.0,
            NEAR_DEPTH.1,
        );
        take(
            &mut self.dz,
            cam.row_depth,
            "row_depth",
            ROW_DEPTH.0,
            ROW_DEPTH.1,
        );
        take(&mut self.horizon, cam.horizon, "horizon", HORIZON.0, HORIZON.1);
        take(
            &mut self.cell_w,
            cam.cell_width,
            "cell_width",
            CELL_WIDTH.0,
            CELL_WIDTH.1,
        );
    }

    /// Overlay a skin's `[creature_field.solver]` onto the placement
    /// tunables, same discipline as `apply_camera`: unset keys keep their
    /// current value, garbage clamps and logs, never drops the widget.
    pub fn apply_solver(&mut self, sol: &crate::config::skins::CreatureFieldSolver) {
        fn take(slot: &mut f32, value: Option<f32>, key: &str, lo: f32, hi: f32) {
            let Some(v) = value else { return };
            if !v.is_finite() {
                tracing::warn!("[creature_field.solver] {key} is not a number; keeping {slot}");
                return;
            }
            let c = v.clamp(lo, hi);
            if c != v {
                tracing::warn!(
                    "[creature_field.solver] {key} = {v} out of range {lo}..={hi}; using {c}"
                );
            }
            *slot = c;
        }
        let s = &mut self.solver;
        match sol.zone.as_deref() {
            None => {}
            Some("ellipse") => s.zone_on = true,
            Some("grid") => s.zone_on = false,
            Some(other) => {
                tracing::warn!(
                    "[creature_field.solver] zone = {other:?} (want \"ellipse\" or \"grid\"); keeping current"
                );
            }
        }
        take(&mut s.zone_inset, sol.zone_inset, "zone_inset", 0.0, 0.45);
        take(&mut s.centre_pull, sol.centre_pull, "centre_pull", 0.0, 5.0);
        if let Some(n) = sol.depth_samples {
            let c = n.clamp(1, 41);
            if c != n {
                tracing::warn!(
                    "[creature_field.solver] depth_samples = {n} out of range 1..=41; using {c}"
                );
            }
            s.depth_samples = c;
        }
        take(&mut s.depth_jitter, sol.depth_jitter, "depth_jitter", 0.0, 1.0);
        take(
            &mut s.lateral_jitter,
            sol.lateral_jitter,
            "lateral_jitter",
            0.0,
            1.0,
        );
        take(&mut s.depth_spread, sol.depth_spread, "depth_spread", 0.0, 5.0);
        take(
            &mut s.row_band_push,
            sol.row_band_push,
            "row_band_push",
            0.0,
            10.0,
        );
        take(&mut s.row_band_px, sol.row_band_px, "row_band_px", 1.0, 200.0);
        take(
            &mut s.occlusion_cap,
            sol.occlusion_cap,
            "occlusion_cap",
            0.0,
            0.95,
        );
        take(&mut s.variation, sol.variation, "variation", 0.0, 2.0);
        if let Some(v) = sol.seed_front {
            s.seed_front = v;
        }
        take(&mut s.fall_reserve, sol.fall_reserve, "fall_reserve", 0.0, 5.0);
        if let Some(v) = sol.fall_reserve_hard {
            s.fall_reserve_hard = v;
        }
        match sol.separation_basis.as_deref() {
            None => {}
            Some("contact") => s.separation_basis = SeparationBasis::Contact,
            Some("card") => s.separation_basis = SeparationBasis::Card,
            Some(other) => {
                tracing::warn!(
                    "[creature_field.solver] separation_basis = {other:?} (want \"contact\" or \"card\"); keeping current"
                );
            }
        }
        if let Some(n) = sol.relax_steps {
            let c = n.min(10);
            if c != n {
                tracing::warn!(
                    "[creature_field.solver] relax_steps = {n} out of range 0..=10; using {c}"
                );
            }
            s.relax_steps = c;
        }
        if let Some(v) = sol.shuffle_ties {
            s.shuffle_ties = v;
        }
    }

    /// Snapshot the camera as authoring vocabulary — every key explicit,
    /// so a scene saved from these params reproduces them exactly. Inverse
    /// of `apply_camera` over in-range values.
    pub fn to_camera(&self) -> crate::config::skins::CreatureFieldCamera {
        crate::config::skins::CreatureFieldCamera {
            focal: Some(self.focal),
            eye_height: Some(self.cam_h),
            near_depth: Some(self.z0),
            row_depth: Some(self.dz),
            horizon: Some(self.horizon),
            cell_width: Some(self.cell_w),
        }
    }

    /// Snapshot the placement tunables as authoring vocabulary; inverse of
    /// `apply_solver` over in-range values.
    pub fn to_solver_toml(&self) -> crate::config::skins::CreatureFieldSolver {
        let s = &self.solver;
        crate::config::skins::CreatureFieldSolver {
            zone: Some(if s.zone_on { "ellipse" } else { "grid" }.to_string()),
            zone_inset: Some(s.zone_inset),
            centre_pull: Some(s.centre_pull),
            depth_samples: Some(s.depth_samples),
            depth_jitter: Some(s.depth_jitter),
            lateral_jitter: Some(s.lateral_jitter),
            depth_spread: Some(s.depth_spread),
            row_band_push: Some(s.row_band_push),
            row_band_px: Some(s.row_band_px),
            occlusion_cap: Some(s.occlusion_cap),
            variation: Some(s.variation),
            seed_front: Some(s.seed_front),
            fall_reserve: Some(s.fall_reserve),
            fall_reserve_hard: Some(s.fall_reserve_hard),
            separation_basis: Some(
                match s.separation_basis {
                    SeparationBasis::Contact => "contact",
                    SeparationBasis::Card => "card",
                }
                .to_string(),
            ),
            relax_steps: Some(s.relax_steps),
            shuffle_ties: Some(s.shuffle_ties),
        }
    }
}

impl CreatureField {
    pub fn new(params: FieldParams) -> Self {
        Self {
            params,
            cols: vec![-1, 0, 1],
            units: Vec::new(),
            next_id: 1,
            obstacles: Vec::new(),
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

    /// With the inscribed zone on, the ellipse supplies the visible border
    /// and every column is usable — which is the point: the floor gets
    /// used. Zone off, the outermost column each side reverts to margin so
    /// nothing sits flush against the floor's edge.
    fn usable_cols(&self) -> Vec<i32> {
        if self.params.solver.zone_on || self.cols.len() <= 2 {
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

    /// Grow symmetrically, one column each side, keeping the vanishing
    /// point centred.
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
    /// (or outside) the new edge.
    fn shrink_floor(&mut self) -> bool {
        if self.cols.len() <= 3 {
            return false;
        }
        let lo = self.cols[0];
        let hi = self.cols[self.cols.len() - 1];
        // Zone on: every column is usable, so only the removed pair must
        // be clear. Zone off: the new outermost pair becomes margin too.
        let pad = if self.params.solver.zone_on { 1 } else { 2 };
        if self
            .units
            .iter()
            .any(|u| u.ci < lo + pad || u.ci > hi - pad)
        {
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

    /// Replace the scene-prop exclusion metadata. Placement input only:
    /// nobody moves, nothing redraws, so the generation stays put.
    pub fn set_obstacles(&mut self, obstacles: Vec<Obstacle>) {
        self.obstacles = obstacles;
    }

    /// The authored exclusion metadata (for overlays/tests).
    pub fn obstacles(&self) -> &[Obstacle] {
        &self.obstacles
    }

    /// An obstacle's screen-x exclusion span under the CURRENT effective
    /// projection — the same `mscale * f_eff / z` scale cards and props
    /// draw through, so the span always matches the rendered prop no
    /// matter how the floor has grown or contracted since authoring.
    pub fn obstacle_span(&self, ob: &Obstacle) -> (f32, f32) {
        let (_, px_per_unit) = self.project_ground(ob.x, ob.z);
        (ob.x - ob.left_w * px_per_unit, ob.x + ob.right_w * px_per_unit)
    }

    /// All obstacle spans projected with the current geometry, as
    /// (x0, x1, z) triples. Recomputed before every placement decision —
    /// cheap (a handful of props), and the only way the spans can track
    /// candidate floor expansion inside the solver's own search loop.
    fn projected_obstacles(&self) -> Vec<(f32, f32, f32)> {
        self.obstacles
            .iter()
            .map(|ob| {
                let (x0, x1) = self.obstacle_span(ob);
                (x0.min(x1), x0.max(x1), ob.z)
            })
            .collect()
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

    /// The floor's world depth range: near edge .. far edge.
    pub fn depth_range(&self) -> (f32, f32) {
        (self.depth_at(0.0), self.depth_at(self.params.rows as f32))
    }

    /// Project an arbitrary ground point for scenery: `x` is stage-space
    /// screen x (0..STAGE_W — props author their lateral position directly
    /// on the stage, so the camera never re-aims at them), `z` is world depth
    /// on the camera axis. Returns the foot point plus pixels-per-world-
    /// unit at that depth — the same `mscale * f_eff / z` cards project
    /// their size through, so a prop's world height stays in scale with the
    /// cards around it.
    pub fn project_ground(&self, x: f32, z: f32) -> ((f32, f32), f32) {
        let z = z.max(0.4);
        ((x, self.screen_y(z)), self.mscale() * self.f_eff() / z)
    }

    /// Invert the ground projection: a stage-space point back to the (x, z)
    /// `project_ground` takes, clamped to the stage width and the floor's
    /// depth range. Drag-to-place editors position props with this.
    pub fn ground_from_screen(&self, sx: f32, sy: f32) -> (f32, f32) {
        let (z_near, z_far) = self.depth_range();
        let dy = sy - self.params.horizon;
        // At or above the horizon the projection has no ground solution;
        // clamp to the far edge instead of dividing toward infinity.
        let z = if dy > 1e-3 {
            (self.params.cam_h * self.f_eff()) / dy
        } else {
            z_far
        };
        (sx.clamp(0.0, STAGE_W), z.clamp(z_near, z_far))
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

    /// The FALL ENVELOPE: the union of a creature's two pose rects at one
    /// ground point. This is the footprint the solver actually reserves —
    /// a creature that arrives cleanly beside a standing neighbour must
    /// still not collide with it once either of them goes down.
    fn env_for(
        &self,
        standing: CardSize,
        prone: CardSize,
        ci: i32,
        row: u32,
        off_x: f32,
        off_z: f32,
    ) -> ScreenRect {
        let a = self.rect_for(standing, ci, row, off_x, off_z);
        let b = self.rect_for(prone, ci, row, off_x, off_z);
        ScreenRect {
            x0: a.x0.min(b.x0),
            y0: a.y0.min(b.y0),
            x1: a.x1.max(b.x1),
            y1: a.y1.max(b.y1),
            z: a.z,
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

    // ---- spawn zone -----------------------------------------------------

    /// The ellipse inscribed in the floor's world-space extent, shrunk by
    /// `zone_inset`. Recomputed from the columns every call, so grow and
    /// shrink keep it correct with no cached state. Returns
    /// (centre x, centre z, semi-axis x, semi-axis z).
    fn zone(&self) -> (f32, f32, f32, f32) {
        let lo = *self.cols.first().unwrap();
        let hi = *self.cols.last().unwrap();
        let xl = self.col_left(lo);
        let xr = self.col_left(hi) + self.cell_w();
        let zn = self.depth_at(0.0);
        let zf = self.depth_at(self.params.rows as f32);
        let k = 1.0 - self.params.solver.zone_inset;
        (
            (xl + xr) / 2.0,
            (zn + zf) / 2.0,
            ((xr - xl) / 2.0 * k).max(1e-3),
            ((zf - zn) / 2.0 * k).max(1e-3),
        )
    }

    /// Normalised radius in the spawn ellipse: <= 1 is inside, 0 is dead
    /// centre.
    fn zone_r(&self, wx: f32, wz: f32) -> f32 {
        let (cx, cz, a, b) = self.zone();
        let dx = (wx - cx) / a;
        let dz = (wz - cz) / b;
        (dx * dx + dz * dz).sqrt()
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

    /// Worst identity-region coverage across all units, front-to-back.
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
    /// >= 1.0 means the separation rule holds everywhere. Widths follow the
    /// configured separation basis. Empty/singleton fields report a
    /// comfortable 9.0.
    pub fn min_separation_ratio(&self) -> f32 {
        let mut xs: Vec<(f32, f32)> = self
            .units
            .iter()
            .map(|u| {
                let r = self.rect(u);
                (r.center_x(), self.sep_width(&r, u.size))
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

    /// The width the separation rule measures a card by, on screen.
    fn sep_width(&self, r: &ScreenRect, size: CardSize) -> f32 {
        match self.params.solver.separation_basis {
            SeparationBasis::Contact => r.width() * size.span,
            SeparationBasis::Card => r.width(),
        }
    }

    // ---- mutation -------------------------------------------------------

    /// Place a new creature, leaving every existing unit untouched. Takes
    /// both pose boxes: the standing box is the current pose at arrival,
    /// the pair defines the fall envelope reserved for it (and against
    /// existing units) forever after. Returns the new unit's id.
    pub fn arrive(&mut self, exist: &str, standing: CardSize, prone: CardSize) -> UnitId {
        let placement = self.choose_home(standing, prone, exist, None);
        self.commit_arrival(exist, standing, prone, placement)
    }

    /// Editor drag-to-place: move a unit's ground point to an arbitrary
    /// stage-space position, inverting the foot projection into (ci, row,
    /// off_x, off_z) so the unit lands exactly under the cursor (nearest
    /// row, leftover depth in off_z). Deliberately bypasses separation —
    /// a Studio override, never called during play.
    pub fn place_at(&mut self, exist: &str, sx: f32, sy: f32) {
        let (x, z) = self.ground_from_screen(sx, sy);
        let wx = (x - STAGE_W / 2.0) * z / self.f_eff().max(1e-3);
        let cell = self.cell_w();
        let ci_f = wx / cell;
        let (lo, hi) = (
            *self.cols.first().unwrap_or(&0) as f32,
            *self.cols.last().unwrap_or(&0) as f32,
        );
        let ci = ci_f.round().clamp(lo, hi);
        let row = (((z - self.params.z0) / self.params.dz - 0.5).round())
            .clamp(0.0, self.params.rows.saturating_sub(1) as f32);
        let off_z = z - self.depth_at(row + 0.5);
        if let Some(u) = self
            .units
            .iter_mut()
            .find(|u| u.members.iter().any(|m| m == exist))
        {
            u.ci = ci as i32;
            u.row = row as u32;
            u.off_x = ci_f - ci;
            u.off_z = off_z;
        }
        self.generation += 1;
    }

    /// Update a placed unit's card box in place (pose change: prone/stand).
    /// Position never moves — permanence — so a wide prone box may overlap
    /// a neighbour; the fall envelope reserved that room at arrival.
    pub fn resize(&mut self, exist: &str, size: CardSize) {
        if let Some(u) = self
            .units
            .iter_mut()
            .find(|u| u.members.iter().any(|m| m == exist))
        {
            u.size = size;
        }
    }

    /// Replace a placed unit's pose boxes in place — calibration metadata
    /// arrived (art loaded after the creature did) or was reloaded.
    /// Position NEVER moves (placement stability); the fall envelope
    /// future arrivals reserve against updates, so no hidden stale
    /// reservation lingers. Bumps the generation only when something
    /// actually changed. Returns true on change.
    pub fn recalibrate(
        &mut self,
        exist: &str,
        standing: CardSize,
        prone: CardSize,
        current: CardSize,
    ) -> bool {
        let Some(u) = self
            .units
            .iter_mut()
            .find(|u| u.members.first().map(String::as_str) == Some(exist))
        else {
            return false;
        };
        if u.standing == standing && u.prone == prone && u.size == current {
            return false;
        }
        u.standing = standing;
        u.prone = prone;
        u.size = current;
        self.generation += 1;
        true
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
    pub fn dismount(&mut self, rider: &str, standing: CardSize, prone: CardSize) {
        let Some(idx) = self
            .units
            .iter()
            .position(|u| u.members.iter().any(|m| m == rider) && u.members.len() > 1)
        else {
            return;
        };
        self.units[idx].members.retain(|m| m != rider);
        let anchor = self.foot(&self.units[idx]);
        let placement = self.choose_home(standing, prone, rider, Some(anchor));
        self.commit_arrival(rider, standing, prone, placement);
    }

    fn commit_arrival(
        &mut self,
        exist: &str,
        standing: CardSize,
        prone: CardSize,
        p: Placement,
    ) -> UnitId {
        let id = self.next_id;
        self.next_id += 1;
        self.units.push(Unit {
            id,
            members: vec![exist.to_string()],
            ci: p.ci,
            row: p.row,
            off_x: p.off_x,
            off_z: p.off_z,
            size: standing,
            standing,
            prone,
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
    /// Heights compare STANDING boxes — the pose a card returns to.
    fn height_penalty(&self, ci: i32, row: u32, h: f32) -> u32 {
        self.units
            .iter()
            .filter(|u| u.ci == ci)
            .filter(|u| (u.row > row && u.standing.h < h) || (u.row < row && u.standing.h > h))
            .count() as u32
    }

    /// All (square, offset) candidates for one arrival: every usable
    /// column × row under the occupancy cap, `depth_samples` depth bases
    /// spanning ±0.40 of a row depth plus jitter, 11 lateral offsets
    /// spanning ±0.7·spread plus jitter. Fisher-Yates shuffled so ties
    /// don't systematically resolve to the first square visited.
    fn candidates(&self, standing_h: f32, rng: &mut Rng) -> Vec<Candidate> {
        let s = &self.params.solver;
        let nz = s.depth_samples.max(1) as usize;
        const NX: usize = 11;
        let mut out = Vec::new();
        for ci in self.usable_cols() {
            for row in 0..self.params.rows {
                let occ = self.occupants_of(ci, row);
                if occ >= self.params.per_square.max(1) as usize + 1 {
                    continue;
                }
                let hpen = self.height_penalty(ci, row, standing_h) as f32;
                for a in 0..nz {
                    let base = if nz == 1 {
                        0.0
                    } else {
                        -0.40 + 0.80 * a as f32 / (nz - 1) as f32
                    };
                    let oz = base * self.params.dz
                        + (rng.unit() * 2.0 - 1.0) * self.params.dz * s.depth_jitter * 0.5;
                    if oz.abs() > self.params.dz * 0.44 {
                        continue;
                    }
                    for b in 0..NX {
                        let ox = (-0.7 + 1.4 * b as f32 / (NX - 1) as f32)
                            * self.params.spread.max(0.2)
                            + (rng.unit() * 2.0 - 1.0) * s.lateral_jitter;
                        out.push(Candidate {
                            ci,
                            row,
                            ox,
                            oz,
                            occ,
                            hpen,
                        });
                    }
                }
            }
        }
        if s.shuffle_ties {
            for k in (1..out.len()).rev() {
                let j = ((rng.unit() * (k as f32 + 1.0)) as usize).min(k);
                out.swap(k, j);
            }
        }
        out
    }

    /// Choose a home for one new card. `near` switches to affinity mode
    /// (dismount): passing candidates are ranked by screen distance from
    /// the anchor instead of the arrival score; the hard rules — zone,
    /// occlusion cap, separation, fall envelope — apply in both modes.
    fn choose_home(
        &mut self,
        standing: CardSize,
        prone: CardSize,
        exist: &str,
        near: Option<(f32, f32)>,
    ) -> Placement {
        let s = self.params.solver.clone();
        let mut rng = Rng::new(seed_of(exist));

        // Seed the empty room front and centre. A lone arrival has no
        // occlusion or separation to solve for, so the scored search has
        // nothing to distinguish candidates by and settles on the middle
        // of the zone — visually, a creature stranded at mid-depth. Put it
        // dead centre of the near-row middle square, closest to the
        // camera, and let everyone after it arrange around that anchor.
        if near.is_none() && s.seed_front && self.units.is_empty() {
            let mid = self
                .usable_cols()
                .into_iter()
                .min_by_key(|c| c.abs())
                .unwrap_or(0);
            // The shortcut still honors scene-prop exclusion: a centre
            // seed inside a blocked span falls through to the search.
            // Spans project with the current geometry, same as the search.
            let r = self.rect_for(standing, mid, 0, 0.0, 0.0);
            let blocked = self
                .projected_obstacles()
                .iter()
                .any(|&(x0, x1, z)| r.z > z && r.center_x() >= x0 && r.center_x() <= x1);
            if !blocked {
                return Placement {
                    ci: mid,
                    row: 0,
                    off_x: 0.0,
                    off_z: 0.0,
                    tight: false,
                    score: 0.0,
                };
            }
        }

        // GRADED RELAXATION. The occlusion cap and the fall envelope are
        // hard rules, and hard rules need somewhere to go when the room
        // genuinely runs out. The floor grows first; once it hits max_cols
        // the constraints loosen a notch at a time and the search retries.
        // Without this the solver drops straight to the "loose" fallback
        // the moment growth is exhausted. Loosening in order costs a
        // little overlap; falling through costs a pile-up.
        let mut loose: Option<Placement> = None;
        let mut relax: u32 = 0;
        for _attempt in 0..(self.params.max_cols + s.relax_steps + 2) {
            let env_thr = 0.50 + relax as f32 * 0.15;
            let cap_thr = (s.occlusion_cap * (1.0 + relax as f32 * 0.40)).min(0.95);
            // Obstacle spans under THIS attempt's projection: `grow_floor`
            // between attempts changes `mscale`/`f_eff`, so the spans are
            // re-projected here rather than cached — a placement decision
            // made during candidate floor expansion still uses current
            // geometry. Pure arithmetic from authored metadata: no texture
            // decoding or filesystem reads in the candidate loop.
            let obstacle_spans = self.projected_obstacles();
            // Each neighbour carries its fall envelope, not just its
            // current pose, so an arrival reserves against the room they
            // will need when they go down.
            let others: Vec<Neighbor> = self
                .units
                .iter()
                .map(|u| {
                    let rect = self.rect(u);
                    Neighbor {
                        cw: self.sep_width(&rect, u.size),
                        env: self.env_for(u.standing, u.prone, u.ci, u.row, u.off_x, u.off_z),
                        rect,
                    }
                })
                .collect();
            let mut best: Option<Placement> = None;
            for cd in self.candidates(standing.h, &mut rng) {
                let r = self.rect_for(standing, cd.ci, cd.row, cd.ox, cd.oz);
                if r.x0 < 6.0 || r.x1 > STAGE_W - 6.0 {
                    continue;
                }
                // The foot must fall inside the inscribed spawn zone.
                let wx = (cd.ci as f32 + cd.ox) * self.cell_w();
                let rad = self.zone_r(wx, r.z);
                if s.zone_on && rad > 1.0 {
                    continue;
                }
                // Scene-prop exclusion: never place a creature BEHIND a
                // prop's calibrated span. Hard at every relaxation notch
                // (a creature inside a boulder is never the least-bad
                // answer); in-front and beside candidates pass untouched.
                if obstacle_spans
                    .iter()
                    .any(|&(x0, x1, z)| r.z > z && r.center_x() >= x0 && r.center_x() <= x1)
                {
                    continue;
                }

                // Separation: every unit owns its own screen column.
                // Clearance scales with contact spans, so the rule
                // survives any zoom unchanged.
                let my_cw = self.sep_width(&r, standing);
                let mut slack = f32::INFINITY;
                for o in &others {
                    let gap = (r.center_x() - o.rect.center_x()).abs();
                    let need = self.params.sep_frac * (my_cw + o.cw) / 2.0;
                    slack = slack.min(gap - need);
                }
                let tight = !others.is_empty() && slack < 0.0;

                let mut behind_cov: f32 = 0.0;
                let mut front: Vec<ScreenRect> = Vec::new();
                for o in &others {
                    if o.rect.z < r.z {
                        front.push(o.rect);
                    } else {
                        behind_cov =
                            behind_cov.max(covered_by(&o.rect, std::slice::from_ref(&r)));
                    }
                }
                let cov_new = covered_by(&r, &front);

                // Occlusion is a feasibility bound, not a cost. Depth
                // variety and occlusion are in direct tension — cards side
                // by side on one depth line never overlap, which is
                // exactly why a scored penalty converges on a straight
                // row. Cap it hard, the way separation is capped, and let
                // the depth terms optimise freely inside what remains.
                if cov_new > cap_thr || behind_cov > cap_thr {
                    let os = cov_new.max(behind_cov);
                    if loose.as_ref().is_none_or(|l| os < l.score) {
                        loose = Some(Placement {
                            ci: cd.ci,
                            row: cd.row,
                            off_x: cd.ox,
                            off_z: cd.oz,
                            tight: true,
                            score: os,
                        });
                    }
                    continue;
                }

                let mut score = match near {
                    // Affinity mode: nearest passing square wins.
                    Some((ax, ay)) => {
                        let fx = (r.center_x() - ax).abs();
                        let fy = (r.y1 - ay).abs();
                        (fx * fx + fy * fy).sqrt()
                    }
                    None => {
                        let mut sc = 1.7 * cov_new
                            + 1.3 * behind_cov
                            + 0.95 * cd.occ as f32
                            + 0.18 * cd.ox.abs()
                            + 0.22 * cd.hpen;
                        // Centre pull: radial in the ellipse. Squared, so
                        // the middle is broadly flat and the rim falls off
                        // hard.
                        sc += s.centre_pull * rad * rad;
                        // Depth spread: repel from other creatures' world
                        // depth. Row band: repel from their foot SCREEN y
                        // — the eye reads screen y, not world z, and
                        // perspective compresses far rows together.
                        let mut d_crowd = 0.0;
                        let mut b_crowd = 0.0;
                        for o in &others {
                            let dz = (r.z - o.rect.z) / (self.params.dz * 0.55);
                            d_crowd += (-dz * dz).exp();
                            let dy = (r.y1 - o.rect.y1) / s.row_band_px.max(1.0);
                            b_crowd += (-dy * dy).exp();
                        }
                        sc += s.depth_spread * d_crowd + s.row_band_push * b_crowd;
                        // Variation shuffles which of the ACCEPTABLE homes
                        // is taken — the hard rules stay hard, so it never
                        // buys an unacceptable one.
                        sc += s.variation * (rng.unit() - 0.5) * 0.55;
                        sc
                    }
                };

                // Fall reserve: room for the pose this creature is not in
                // yet, kept free in advance — cheaper than relocating
                // anyone later, and relocation is forbidden outright.
                if s.fall_reserve > 0.0 && !others.is_empty() {
                    let my_env = self.env_for(standing, prone, cd.ci, cd.row, cd.ox, cd.oz);
                    let mut env_pen = 0.0;
                    let mut worst: f32 = 0.0;
                    for o in &others {
                        let ov = overlap_1d(&my_env, &o.env);
                        env_pen += ov;
                        worst = worst.max(ov);
                    }
                    if s.fall_reserve_hard && worst > env_thr {
                        if loose.as_ref().is_none_or(|l| worst < l.score) {
                            loose = Some(Placement {
                                ci: cd.ci,
                                row: cd.row,
                                off_x: cd.ox,
                                off_z: cd.oz,
                                tight: true,
                                score: worst,
                            });
                        }
                        continue;
                    }
                    if near.is_none() {
                        score += s.fall_reserve * env_pen;
                    }
                }

                if tight {
                    let fs = -slack + 0.3 * cov_new;
                    if loose.as_ref().is_none_or(|l| fs < l.score) {
                        loose = Some(Placement {
                            ci: cd.ci,
                            row: cd.row,
                            off_x: cd.ox,
                            off_z: cd.oz,
                            tight: true,
                            score: fs,
                        });
                    }
                    continue;
                }
                if best.as_ref().is_none_or(|b| score < b.score) {
                    best = Some(Placement {
                        ci: cd.ci,
                        row: cd.row,
                        off_x: cd.ox,
                        off_z: cd.oz,
                        tight: false,
                        score,
                    });
                }
            }
            if let Some(best) = best {
                return best;
            }
            if !self.grow_floor() {
                if relax < s.relax_steps {
                    relax += 1;
                    continue;
                }
                break;
            }
        }
        if let Some(loose) = loose {
            return loose;
        }
        // Truly nothing measured: a random usable column, first row that
        // doesn't stand a short card behind a tall one, marked tight.
        let cols = self.usable_cols();
        let ci = cols[((rng.unit() * cols.len() as f32) as usize).min(cols.len() - 1)];
        let row = (0..self.params.rows)
            .find(|&rw| self.height_penalty(ci, rw, standing.h) == 0)
            .unwrap_or(0);
        Placement {
            ci,
            row,
            off_x: 0.0,
            off_z: 0.0,
            tight: true,
            score: 0.0,
        }
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

#[derive(Debug, Clone, Copy)]
struct Candidate {
    ci: i32,
    row: u32,
    ox: f32,
    oz: f32,
    occ: usize,
    hpen: f32,
}

/// One existing unit as the solver sees it: current-pose rect, separation
/// width on that rect, and the fall envelope it reserved at arrival.
struct Neighbor {
    rect: ScreenRect,
    env: ScreenRect,
    cw: f32,
}

/// Horizontal overlap of two rects as a fraction of the narrower one.
fn overlap_1d(a: &ScreenRect, b: &ScreenRect) -> f32 {
    let o = a.x1.min(b.x1) - a.x0.max(b.x0);
    if o <= 0.0 {
        return 0.0;
    }
    o / (a.width().min(b.width())).max(1.0)
}

/// Fraction of `r`'s IDENTITY REGION hidden by any rect in `list` (all
/// assumed nearer, drawn on top). The region derives from the card's own
/// aspect, not from its pose name: the top-62% rule assumes a head sits on
/// top, which is true of an upright humanoid and false of a quadruped — a
/// standing coyote is wider than tall with its head halfway up an edge,
/// and keying off a label would let arrivals bury it and score clean.
fn covered_by(r: &ScreenRect, list: &[ScreenRect]) -> f32 {
    if list.is_empty() {
        return 0.0;
    }
    const NX: usize = 9;
    const NY: usize = 13;
    let aspect = (r.x1 - r.x0) / (r.y1 - r.y0).max(1.0);
    let idf = (0.62 + 0.75 * (aspect - 0.6)).clamp(0.55, 0.95);
    let y_top = r.y0;
    let y_bot = r.y0 + (r.y1 - r.y0) * idf;
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
/// (The prototype adds a per-engagement session salt so two fights in one
/// room differ; deliberately skipped here to keep layouts reproducible —
/// a future knob.)
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
        CardSize::new(0.60, 0.92)
    }
    fn troll() -> CardSize {
        CardSize::new(0.78, 1.52)
    }
    fn spider() -> CardSize {
        CardSize::new(0.92, 0.60)
    }

    /// The prone box the game derives for a biped of this standing size
    /// (see `card_size_for` in creature_cards.rs).
    fn prone_of(s: CardSize) -> CardSize {
        CardSize::new((s.h * 0.90).max(0.35), (s.h * 0.35).max(0.30))
    }

    fn arrive(f: &mut CreatureField, exist: &str, s: CardSize) -> UnitId {
        f.arrive(exist, s, prone_of(s))
    }

    fn world_pos(f: &CreatureField, exist: &str) -> (i32, u32, f32, f32) {
        let u = f.unit_of(exist).unwrap();
        (u.ci, u.row, u.off_x, u.off_z)
    }

    fn fill(f: &mut CreatureField, n: usize) {
        fill_more(f, 0, n);
    }

    fn fill_more(f: &mut CreatureField, from: usize, to: usize) {
        let sizes = [kobold(), troll(), spider()];
        for i in from..to {
            arrive(f, &format!("{}", 1000 + i), sizes[i % 3]);
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

    #[test]
    fn obstacles_block_placement_behind_never_in_front() {
        // A prop at stage-left quarter, depth 3, excluding ~1.57 world
        // units each side (≈ the left half of the stage at 3 columns).
        let obstacle = Obstacle {
            x: STAGE_W / 4.0,
            z: 3.0,
            left_w: 1.6,
            right_w: 1.6,
        };
        let mut f = CreatureField::default();
        f.set_obstacles(vec![obstacle]);
        fill(&mut f, 10);
        // Spans are checked against CURRENT geometry: growth shrinks the
        // projected span, so units placed under an earlier (wider) span
        // still satisfy the current one.
        let (x0, x1) = f.obstacle_span(&obstacle);
        for u in f.units() {
            let r = f.rect(u);
            let behind = r.z > obstacle.z && r.center_x() >= x0 && r.center_x() <= x1;
            assert!(
                !behind,
                "unit at center_x {} z {} placed behind the obstacle span {x0}..{x1}",
                r.center_x(),
                r.z
            );
        }
        assert_eq!(f.units().len(), 10, "everyone still got placed");
    }

    #[test]
    fn seed_front_shortcut_honors_obstacles() {
        // Whole stage blocked behind z = 0: the centre seed would violate
        // it, so the first arrival must land through the search instead.
        let obstacle = Obstacle {
            x: STAGE_W / 2.0,
            z: 0.0,
            left_w: 50.0,
            right_w: 50.0,
        };
        let mut f = CreatureField::default();
        f.set_obstacles(vec![obstacle]);
        // Everything is "behind" z=0, so every candidate is rejected and
        // placement degrades to the least-bad fallback — the field must
        // not panic and must still place the unit.
        f.arrive("#1", CardSize::default(), CardSize::default());
        assert_eq!(f.units().len(), 1);
    }

    /// The proposal's worked example (finding 3): a unit-height prop at
    /// depth 4 projects to 105px at 3 columns and 38.89px at 11 — the
    /// exclusion span must follow the effective projection, not a cached
    /// pixel width.
    #[test]
    fn obstacle_span_tracks_floor_growth_and_contraction() {
        let ob = Obstacle {
            x: STAGE_W / 2.0,
            z: 4.0,
            left_w: 0.5,
            right_w: 0.5,
        };
        let mut f = CreatureField::default();
        f.set_obstacles(vec![ob]);
        let width = |f: &CreatureField| {
            let (x0, x1) = f.obstacle_span(&ob);
            x1 - x0
        };
        assert!((width(&f) - 105.0).abs() < 1e-3, "3 cols: {}", width(&f));
        while f.columns().len() < 11 {
            assert!(f.grow_floor());
        }
        assert!(
            (width(&f) - 38.89).abs() < 0.05,
            "11 cols: {}",
            width(&f)
        );
        // Contraction restores the original projection exactly.
        f.contract_floor();
        assert_eq!(f.columns().len(), 3);
        assert!((width(&f) - 105.0).abs() < 1e-3);
    }

    /// A new arrival AFTER floor growth/contraction is validated against
    /// the current projected span — including growth triggered while its
    /// own placement search runs — and departures that contract the floor
    /// keep later arrivals honest too.
    #[test]
    fn arrivals_use_current_obstacle_projection_across_growth() {
        let ob = Obstacle {
            x: STAGE_W / 2.0,
            z: 3.2,
            left_w: 1.2,
            right_w: 1.2,
        };
        let mut f = CreatureField::default();
        f.set_obstacles(vec![ob]);
        // Enough arrivals to force growth (search-loop growth included).
        fill(&mut f, 14);
        assert!(f.columns().len() > 3, "population must have grown the floor");
        let check = |f: &CreatureField, who: &str| {
            let (x0, x1) = f.obstacle_span(&ob);
            for u in f.units() {
                let r = f.rect(u);
                assert!(
                    !(r.z > ob.z && r.center_x() >= x0 && r.center_x() <= x1),
                    "{who}: unit behind current span {x0}..{x1} (cx {}, z {})",
                    r.center_x(),
                    r.z
                );
            }
        };
        check(&f, "after growth");
        // Loot most of the room: the floor contracts, the span widens back
        // out, and the next arrival must be checked against the WIDE span.
        for i in 3..14 {
            f.depart(&format!("{}", 1000 + i));
        }
        assert_eq!(f.columns().len(), 3, "floor contracted");
        arrive(&mut f, "latecomer", kobold());
        let r = f.rect(f.unit_of("latecomer").unwrap());
        let (x0, x1) = f.obstacle_span(&ob);
        assert!(
            !(r.z > ob.z && r.center_x() >= x0 && r.center_x() <= x1),
            "latecomer placed behind the re-widened span"
        );
    }

    /// Fixed camera, fixed crowd scale: equal creatures at depths 3 and 6
    /// project at a 2:1 apparent-height ratio, and doubling the world
    /// height exactly compensates doubling the depth (finding 6's
    /// calibration invariants).
    #[test]
    fn depth_ratio_and_inverse_compensation_hold() {
        let f = CreatureField::default();
        let h_at = |size: CardSize, oz: f32| {
            // rect_for at ci 0 row 0 with off_z chosen to hit the depth.
            let r = f.rect_for(size, 0, 0, 0.0, oz);
            r.y1 - r.y0
        };
        let unit = CardSize::new(0.5, 1.0);
        let z_row = f.depth_at(0.5); // rect_for's base depth
        let near = h_at(unit, 3.0 - z_row);
        let far = h_at(unit, 6.0 - z_row);
        assert!((near / far - 2.0).abs() < 1e-4, "ratio {}", near / far);
        let tall_far = h_at(CardSize::new(0.5, 2.0), 6.0 - z_row);
        assert!((tall_far - near).abs() < 1e-3, "{tall_far} vs {near}");
    }

    /// Calibration arriving late (art loaded after placement) updates the
    /// reservation WITHOUT moving anyone — placement stability — and only
    /// dirties the generation when something changed.
    #[test]
    fn recalibrate_updates_boxes_without_moving_units() {
        let mut f = CreatureField::default();
        arrive(&mut f, "a", kobold());
        arrive(&mut f, "b", troll());
        let pos = world_pos(&f, "a");
        let g = f.generation;
        let wide = CardSize::new(1.6, 0.8);
        assert!(f.recalibrate("a", wide, prone_of(wide), wide));
        assert_eq!(world_pos(&f, "a"), pos, "recalibration must not move");
        assert!(f.generation > g);
        let u = f.unit_of("a").unwrap();
        assert_eq!(u.standing, wide);
        assert_eq!(u.size, wide);
        // Idempotent: same boxes again is a no-op.
        let g = f.generation;
        assert!(!f.recalibrate("a", wide, prone_of(wide), wide));
        assert_eq!(f.generation, g);
        // Unknown creature: no-op, no panic.
        assert!(!f.recalibrate("ghost", wide, wide, wide));
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
    fn separation_and_occlusion_hold_at_moderate_population() {
        let mut f = CreatureField::default();
        fill(&mut f, 7); // the prototype's default room
        assert!(
            f.min_separation_ratio() >= 1.0,
            "separation ratio {} < 1.0",
            f.min_separation_ratio()
        );
        assert!(
            f.worst_coverage() <= f.params.solver.occlusion_cap + 1e-3,
            "worst coverage {} over cap {}",
            f.worst_coverage(),
            f.params.solver.occlusion_cap
        );
        assert!(f.units().iter().all(|u| !u.tight));
    }

    /// The zone supplies the border the old margin columns used to: every
    /// foot lands inside the inscribed ellipse, so nothing sits flush
    /// against the floor's edge — while the whole floor stays usable.
    #[test]
    fn zone_excludes_rim_placements() {
        let mut f = CreatureField::default();
        fill(&mut f, 9);
        let lo = *f.columns().first().unwrap();
        let hi = *f.columns().last().unwrap();
        assert_eq!(lo, -hi, "growth must stay centred");
        for u in f.units() {
            let wx = (u.ci as f32 + u.off_x) * f.cell_w();
            let rad = f.zone_r(wx, f.ground_z(u));
            assert!(
                rad <= 1.0 + 1e-3,
                "unit at normalised radius {rad} — outside the spawn zone"
            );
        }
    }

    /// First arrival into an empty field seeds the near-row middle square,
    /// dead centre — not a scored spot in the middle of the zone.
    #[test]
    fn empty_field_seeds_front_and_centre() {
        let mut f = CreatureField::default();
        arrive(&mut f, "first", kobold());
        let u = f.unit_of("first").unwrap();
        assert_eq!((u.ci, u.row), (0, 0));
        assert_eq!((u.off_x, u.off_z), (0.0, 0.0));
        assert!(!u.tight);
    }

    /// The fall envelope reserves room in advance: a pair with very wide
    /// prone boxes must end up farther apart than their standing cards
    /// alone would require, and their envelopes must respect the hard
    /// overlap bound (0.50 at relax 0).
    #[test]
    fn fall_envelope_reserves_room() {
        let standing = CardSize::new(0.5, 1.4);
        let wide_prone = CardSize::new(2.2, 0.4);
        let mut f = CreatureField::default();
        f.arrive("a", standing, wide_prone);
        f.arrive("b", standing, wide_prone);
        let ua = f.unit_of("a").unwrap().clone();
        let ub = f.unit_of("b").unwrap().clone();
        let env_a = f.env_for(ua.standing, ua.prone, ua.ci, ua.row, ua.off_x, ua.off_z);
        let env_b = f.env_for(ub.standing, ub.prone, ub.ci, ub.row, ub.off_x, ub.off_z);
        assert!(
            overlap_1d(&env_a, &env_b) <= 0.50 + 1e-3,
            "envelope overlap {} over the hard bound",
            overlap_1d(&env_a, &env_b)
        );
        // Control: the same standing cards with narrow prone boxes are
        // allowed to sit closer; the wide-prone pair must sit at least as
        // far apart as the envelope demands over the card demand.
        let d_wide = (f.foot(&ua).0 - f.foot(&ub).0).abs();
        let mut g = CreatureField::default();
        g.arrive("a", standing, standing);
        g.arrive("b", standing, standing);
        let ga = g.foot(g.unit_of("a").unwrap()).0;
        let gb = g.foot(g.unit_of("b").unwrap()).0;
        let d_narrow = (ga - gb).abs();
        assert!(
            d_wide + 1e-3 >= d_narrow,
            "wide-prone pair ({d_wide}px) packed tighter than narrow control ({d_narrow}px)"
        );
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

    /// Determinism: the RNG is seeded per creature from its exist id, so
    /// the same roster in the same arrival order lays out identically.
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
        assert!(
            zs.windows(2).all(|w| w[0] >= w[1]),
            "not far-to-near: {zs:?}"
        );
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
        arrive(&mut f, "mount1", troll());
        arrive(&mut f, "rider1", kobold());
        arrive(&mut f, "bystander", spider());
        let mount_home = world_pos(&f, "mount1");
        f.mount("rider1", "mount1");
        assert_eq!(f.units().len(), 2);
        let pair = f.unit_of("rider1").unwrap();
        assert_eq!(
            pair.members,
            vec!["mount1", "rider1"],
            "mount stays members[0]"
        );
        assert_eq!(world_pos(&f, "mount1"), mount_home, "mount must not move");
        assert_eq!(world_pos(&f, "bystander").0, world_pos(&f, "bystander").0);
    }

    #[test]
    fn dismount_places_rider_near_the_mount() {
        let mut f = CreatureField::default();
        arrive(&mut f, "mount1", troll());
        f.mount("rider1", "mount1"); // rider joins without own square
                                     // Crowd the room so nearness is actually contested.
        fill_more(&mut f, 0, 5);
        let mount_foot = f.foot(f.unit_of("mount1").unwrap());
        f.dismount("rider1", kobold(), prone_of(kobold()));
        let rider = f.unit_of("rider1").unwrap();
        assert_eq!(rider.members, vec!["rider1"]);
        let rider_foot = f.foot(rider);
        // Near: the rider must land closer to the mount than the farthest
        // possible square (half the stage), and pass the hard rules.
        let d =
            ((rider_foot.0 - mount_foot.0).powi(2) + (rider_foot.1 - mount_foot.1).powi(2)).sqrt();
        assert!(d < STAGE_W / 2.0, "rider flung {d}px from its mount");
        assert!(!rider.tight, "affinity placement must honour separation");
        assert!(
            f.worst_coverage() <= f.params.solver.occlusion_cap + 1e-3,
            "dismount broke the occlusion cap: {} (rider tight={}, cols={})",
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
        arrive(&mut f, "mount1", troll());
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
        arrive(&mut f, "mount1", troll());
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
        arrive(&mut f, "a", kobold());
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
            assert!(u.ci >= lo && u.ci <= hi);
        }
    }

    /// Scenery projection: ground_from_screen inverts project_ground for
    /// any point inside the floor's depth range, and clamps outside it.
    #[test]
    fn ground_projection_roundtrips_and_clamps() {
        let f = CreatureField::default();
        let (z_near, z_far) = f.depth_range();
        for z in [z_near, (z_near + z_far) / 2.0, z_far] {
            for x in [0.0, 220.0, STAGE_W] {
                let ((sx, sy), scale) = f.project_ground(x, z);
                assert!(scale > 0.0);
                let (rx, rz) = f.ground_from_screen(sx, sy);
                assert!((rx - x).abs() < 1e-3, "x {x} -> {rx}");
                assert!((rz - z).abs() < 1e-3, "z {z} -> {rz}");
            }
        }
        // Above the horizon: clamps to the far edge instead of exploding.
        let (_, z) = f.ground_from_screen(100.0, f.params.horizon - 50.0);
        assert_eq!(z, z_far);
        // Off-stage x clamps into the stage.
        let (x, _) = f.ground_from_screen(-40.0, STAGE_H - 10.0);
        assert_eq!(x, 0.0);
    }

    #[test]
    fn sanitize_camera_shares_loader_limits() {
        use camera_limits::*;
        let d = FieldParams::default();

        // Valid params are untouched.
        let mut p = FieldParams::default();
        assert!(!p.sanitize_camera(&d));
        assert_eq!(p, d);

        // Exact boundaries are valid on both ends.
        let mut p = FieldParams::default();
        p.focal = FOCAL.0;
        p.cam_h = EYE_HEIGHT.1;
        p.z0 = NEAR_DEPTH.0;
        p.dz = ROW_DEPTH.1;
        p.horizon = HORIZON.0;
        p.cell_w = CELL_WIDTH.1;
        assert!(!p.sanitize_camera(&d));
        assert_eq!(p.focal, FOCAL.0);
        assert_eq!(p.cell_w, CELL_WIDTH.1);

        // Out-of-range clamps to the SAME bounds apply_camera uses — a
        // previewed focal=20 becomes 60 in the editor, not on reload.
        let mut p = FieldParams::default();
        p.focal = 20.0;
        p.z0 = 0.0; // zero geometry: invalid projection while editing
        p.dz = -1.0;
        p.cell_w = 999.0;
        assert!(p.sanitize_camera(&d));
        assert_eq!(p.focal, FOCAL.0);
        assert_eq!(p.z0, NEAR_DEPTH.0);
        assert_eq!(p.dz, ROW_DEPTH.0);
        assert_eq!(p.cell_w, CELL_WIDTH.1);

        // Nonfinite falls back to the caller's previous value...
        let mut fallback = FieldParams::default();
        fallback.focal = 500.0;
        let mut p = fallback.clone();
        p.focal = f32::NAN;
        p.horizon = f32::INFINITY;
        assert!(p.sanitize_camera(&fallback));
        assert_eq!(p.focal, 500.0);
        assert_eq!(p.horizon, fallback.horizon);
        // ...and to the built-in default when the fallback is bad too.
        let mut bad_fallback = FieldParams::default();
        bad_fallback.cam_h = f32::NAN;
        let mut p = FieldParams::default();
        p.cam_h = f32::NAN;
        assert!(p.sanitize_camera(&bad_fallback));
        assert_eq!(p.cam_h, d.cam_h);

        // Sanitized params round-trip: what the editor shows is exactly
        // what serialize -> apply_camera reproduces.
        let mut p = FieldParams::default();
        p.focal = 20.0;
        p.cam_h = 30.0;
        p.sanitize_camera(&d);
        let mut reloaded = FieldParams::default();
        reloaded.apply_camera(&p.to_camera());
        assert_eq!(reloaded.focal, p.focal);
        assert_eq!(reloaded.cam_h, p.cam_h);
        assert_eq!(reloaded.z0, p.z0);
        assert_eq!(reloaded.dz, p.dz);
        assert_eq!(reloaded.horizon, p.horizon);
        assert_eq!(reloaded.cell_w, p.cell_w);
    }

    #[test]
    fn skin_camera_overlays_defaults_and_clamps_bad_values() {
        use crate::config::skins::CreatureFieldCamera;
        let d = FieldParams::default();

        // Unset keys keep the built-in defaults.
        let mut p = FieldParams::default();
        p.apply_camera(&CreatureFieldCamera::default());
        assert_eq!(p, d);

        // Set keys land on the right solver fields (the TOML vocabulary
        // deliberately differs from the short field names).
        let mut p = FieldParams::default();
        p.apply_camera(&CreatureFieldCamera {
            focal: Some(300.0),
            eye_height: Some(2.0),
            near_depth: Some(3.0),
            row_depth: Some(1.0),
            horizon: Some(120.0),
            cell_width: Some(1.4),
        });
        assert_eq!(
            (p.focal, p.cam_h, p.z0, p.dz, p.horizon, p.cell_w),
            (300.0, 2.0, 3.0, 1.0, 120.0, 1.4)
        );

        // A garbage focal degrades to the nearest bound, never panics and
        // never leaves the field unusable.
        let mut p = FieldParams::default();
        p.apply_camera(&CreatureFieldCamera {
            focal: Some(0.0),
            ..Default::default()
        });
        assert!(p.focal >= 60.0, "focal clamped up, got {}", p.focal);

        // NaN is ignored outright rather than poisoning the projection.
        let mut p = FieldParams::default();
        p.apply_camera(&CreatureFieldCamera {
            eye_height: Some(f32::NAN),
            ..Default::default()
        });
        assert_eq!(p.cam_h, d.cam_h);
    }

    #[test]
    fn skin_solver_overlays_defaults_and_clamps_bad_values() {
        use crate::config::skins::CreatureFieldSolver;
        let d = FieldParams::default();

        // Unset keys keep the built-in defaults.
        let mut p = FieldParams::default();
        p.apply_solver(&CreatureFieldSolver::default());
        assert_eq!(p, d);

        // Set keys land on the right tunables, including the two string
        // vocabularies.
        let mut p = FieldParams::default();
        p.apply_solver(&CreatureFieldSolver {
            zone: Some("grid".into()),
            zone_inset: Some(0.2),
            centre_pull: Some(1.0),
            depth_samples: Some(5),
            depth_jitter: Some(0.1),
            lateral_jitter: Some(0.05),
            depth_spread: Some(0.3),
            row_band_push: Some(2.0),
            row_band_px: Some(40.0),
            occlusion_cap: Some(0.25),
            variation: Some(0.1),
            seed_front: Some(false),
            fall_reserve: Some(1.2),
            fall_reserve_hard: Some(false),
            separation_basis: Some("card".into()),
            relax_steps: Some(2),
            shuffle_ties: Some(false),
        });
        let s = &p.solver;
        assert!(!s.zone_on);
        assert_eq!(s.separation_basis, SeparationBasis::Card);
        assert_eq!(
            (s.zone_inset, s.centre_pull, s.depth_samples, s.relax_steps),
            (0.2, 1.0, 5, 2)
        );
        assert!(!s.seed_front && !s.fall_reserve_hard && !s.shuffle_ties);

        // Out-of-range clamps; unknown vocabulary keeps the current value.
        let mut p = FieldParams::default();
        p.apply_solver(&CreatureFieldSolver {
            occlusion_cap: Some(5.0),
            relax_steps: Some(99),
            zone: Some("hexes".into()),
            separation_basis: Some("feet".into()),
            depth_jitter: Some(f32::NAN),
            ..Default::default()
        });
        assert_eq!(p.solver.occlusion_cap, 0.95);
        assert_eq!(p.solver.relax_steps, 10);
        assert!(p.solver.zone_on, "unknown zone word keeps the default");
        assert_eq!(p.solver.separation_basis, SeparationBasis::Contact);
        assert_eq!(p.solver.depth_jitter, d.solver.depth_jitter);
    }
}
