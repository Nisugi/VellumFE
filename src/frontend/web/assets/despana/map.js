/**
 * Pure viewport state for the Despana desktop map.
 *
 * The canvas adapter owns DOM events and drawing. This model owns camera math,
 * one-pointer drag state, wire-shape normalization, and the recenter invariant:
 * incoming data recenters only when the actual mapped room or ghost cell
 * changes. Scene refreshes and other map-state changes preserve the camera.
 */

const EMPTY_LIST = Object.freeze([]);
const DEFAULT_PIXELS_PER_CELL = 22;
const DEFAULT_MIN_PIXELS_PER_CELL = 4;
const DEFAULT_MAX_PIXELS_PER_CELL = 64;
const WHEEL_ZOOM_FACTOR = 1.15;

export class DesktopMapViewportError extends Error {
  constructor(code, message) {
    super(message);
    this.name = "DesktopMapViewportError";
    this.code = code;
  }
}

export class DesktopMapViewport {
  constructor({
    pixelsPerCell = DEFAULT_PIXELS_PER_CELL,
    minPixelsPerCell = DEFAULT_MIN_PIXELS_PER_CELL,
    maxPixelsPerCell = DEFAULT_MAX_PIXELS_PER_CELL,
  } = {}) {
    requirePositiveFinite(minPixelsPerCell, "minPixelsPerCell");
    requirePositiveFinite(maxPixelsPerCell, "maxPixelsPerCell");
    requirePositiveFinite(pixelsPerCell, "pixelsPerCell");
    if (minPixelsPerCell > maxPixelsPerCell) {
      throw new DesktopMapViewportError(
        "scale-range",
        "minPixelsPerCell must not exceed maxPixelsPerCell",
      );
    }

    this._minPixelsPerCell = minPixelsPerCell;
    this._maxPixelsPerCell = maxPixelsPerCell;
    this._sceneSource = undefined;
    this._scene = null;
    this._state = normalizeState(null);
    this._camera = {
      x: 0,
      y: 0,
      pixelsPerCell: clamp(
        pixelsPerCell,
        minPixelsPerCell,
        maxPixelsPerCell,
      ),
    };
    this._locationIdentity = null;
    this._centeredIdentity = null;
    this._drag = null;
    this._snapshot = null;
    this._publish();
  }

  /** Return the current frozen view. Large scene arrays retain their identity. */
  snapshot() {
    return this._snapshot;
  }

  /**
   * Replace the current protocol frame.
   *
   * `state` accepts either wire names (`in_ghost`, `ghost_edges`) or their
   * camel-case desktop equivalents. Invalid/missing optional fields normalize
   * to safe empty values. A usable changed room/cell identity recenters once.
   */
  setFrame({ scene = null, state = null } = {}) {
    if (scene !== this._sceneSource) {
      this._sceneSource = scene;
      this._scene = normalizeScene(scene);
    }
    this._state = normalizeState(state);

    const identity = locationIdentity(this._state, this._scene);
    this._locationIdentity = identity;
    if (identity === null) this._centeredIdentity = null;
    if (
      identity !== null &&
      identity !== this._centeredIdentity &&
      this._centerOnCurrentCell()
    ) {
      this._centeredIdentity = identity;
    }

    return this._publish();
  }

  /** Recenter explicitly. Returns false when the frame has no usable cell. */
  center() {
    const centered = this._centerOnCurrentCell();
    if (centered) {
      this._centeredIdentity = this._locationIdentity;
      this._publish();
    }
    return centered;
  }

  /** Start one drag. Additional pointers are ignored; pinch is not supported. */
  beginDrag({ pointerId = 0, x, y } = {}) {
    if (!isFiniteNumber(x) || !isFiniteNumber(y)) return false;
    if (this._drag && this._drag.pointerId !== pointerId) return false;
    this._drag = { pointerId, x, y };
    this._publish();
    return true;
  }

  /** Pan by the active pointer's pixel delta. Returns false for other pointers. */
  dragTo({ pointerId = 0, x, y } = {}) {
    if (
      !this._drag ||
      this._drag.pointerId !== pointerId ||
      !isFiniteNumber(x) ||
      !isFiniteNumber(y)
    ) {
      return false;
    }

    const dx = x - this._drag.x;
    const dy = y - this._drag.y;
    this._drag = { pointerId, x, y };
    if (dx === 0 && dy === 0) return false;

    this._camera.x -= dx / this._camera.pixelsPerCell;
    this._camera.y -= dy / this._camera.pixelsPerCell;
    this._publish();
    return true;
  }

  /** End the active drag. Pointer cancellation uses the same operation. */
  endDrag(pointerId = 0) {
    if (!this._drag || this._drag.pointerId !== pointerId) return false;
    this._drag = null;
    this._publish();
    return true;
  }

  /**
   * Apply one wheel notch and keep the world cell beneath the pointer fixed.
   * Omit viewport geometry to zoom around the viewport center. Scale is
   * clamped; malformed or zero deltas are ignored.
   */
  zoomWheel({ deltaY, x, y, width, height } = {}) {
    if (!isFiniteNumber(deltaY) || deltaY === 0) return false;

    const oldScale = this._camera.pixelsPerCell;
    const factor = deltaY < 0 ? WHEEL_ZOOM_FACTOR : 1 / WHEEL_ZOOM_FACTOR;
    const newScale = clamp(
      oldScale * factor,
      this._minPixelsPerCell,
      this._maxPixelsPerCell,
    );
    if (newScale === oldScale) return false;

    const hasAnchor =
      isFiniteNumber(x) &&
      isFiniteNumber(y) &&
      isFiniteNumber(width) &&
      isFiniteNumber(height) &&
      width > 0 &&
      height > 0;
    const offsetX = hasAnchor ? x - width / 2 : 0;
    const offsetY = hasAnchor ? y - height / 2 : 0;
    const anchorCellX = this._camera.x + offsetX / oldScale;
    const anchorCellY = this._camera.y + offsetY / oldScale;

    this._camera.pixelsPerCell = newScale;
    this._camera.x = anchorCellX - offsetX / newScale;
    this._camera.y = anchorCellY - offsetY / newScale;
    this._publish();
    return true;
  }

  /**
   * Resolve a viewport point to the nearest generated-map room.
   *
   * The result is the mapdb room id used by `.go2`, or `null` when the point
   * is outside the click target. Keeping this transform beside the camera
   * state prevents the DOM adapter from maintaining a second copy of the
   * map-coordinate math.
   */
  roomAtViewportPoint({ x, y, width, height, slopPixels = 14 } = {}) {
    if (
      !this._scene ||
      !isFiniteNumber(x) ||
      !isFiniteNumber(y) ||
      !isFiniteNumber(width) ||
      !isFiniteNumber(height) ||
      width <= 0 ||
      height <= 0 ||
      !isFiniteNumber(slopPixels) ||
      slopPixels < 0
    ) {
      return null;
    }

    const worldX = this._camera.x + (x - width / 2) / this._camera.pixelsPerCell;
    const worldY = this._camera.y + (y - height / 2) / this._camera.pixelsPerCell;
    let best = null;
    let bestDistance = Infinity;
    for (const room of this._scene.rooms) {
      if (!Number.isSafeInteger(room?.i) || !isFiniteNumber(room.x) || !isFiniteNumber(room.y)) {
        continue;
      }
      const distance = Math.hypot(room.x - worldX, room.y - worldY);
      if (distance < bestDistance) {
        best = room.i;
        bestDistance = distance;
      }
    }
    const slopCells = Math.max(0.55, slopPixels / this._camera.pixelsPerCell);
    return best !== null && bestDistance <= slopCells ? best : null;
  }

  _centerOnCurrentCell() {
    if (!this._state.cell) return false;
    this._camera.x = this._state.cell[0];
    this._camera.y = this._state.cell[1];
    return true;
  }

  _publish() {
    this._snapshot = Object.freeze({
      scene: this._scene,
      state: this._state,
      camera: Object.freeze({ ...this._camera }),
      locationIdentity: this._locationIdentity,
      dragging: this._drag !== null,
    });
    return this._snapshot;
  }
}

function normalizeScene(scene) {
  if (!scene || typeof scene !== "object" || Array.isArray(scene)) return null;
  return Object.freeze({
    location: typeof scene.location === "string" ? scene.location : "",
    sheet: typeof scene.sheet === "string" ? scene.sheet : "",
    rooms: Array.isArray(scene.rooms) ? scene.rooms : EMPTY_LIST,
    edges: Array.isArray(scene.edges) ? scene.edges : EMPTY_LIST,
    labels: Array.isArray(scene.labels) ? scene.labels : EMPTY_LIST,
  });
}

function normalizeState(state) {
  const value = state && typeof state === "object" && !Array.isArray(state) ? state : {};
  const room = Number.isSafeInteger(value.room) && value.room >= 0 ? value.room : null;
  const cell = normalizeCell(value.cell);
  const inGhost = value.inGhost === true || value.in_ghost === true;
  const ghostEdges = Array.isArray(value.ghostEdges)
    ? value.ghostEdges
    : Array.isArray(value.ghost_edges)
      ? value.ghost_edges
      : EMPTY_LIST;

  return Object.freeze({
    available: value.available === true,
    location: typeof value.location === "string" ? value.location : null,
    room,
    cell,
    inGhost,
    ghosts: Array.isArray(value.ghosts) ? value.ghosts : EMPTY_LIST,
    ghostEdges,
    travel: value.travel && typeof value.travel === "object" ? value.travel : null,
  });
}

function normalizeCell(cell) {
  if (
    !Array.isArray(cell) ||
    cell.length < 2 ||
    !isFiniteNumber(cell[0]) ||
    !isFiniteNumber(cell[1])
  ) {
    return null;
  }
  return Object.freeze([cell[0], cell[1]]);
}

function locationIdentity(state, scene) {
  if (!state.cell) return null;
  if (!state.inGhost && state.room !== null) return `room:${state.room}`;
  const location = state.location ?? scene?.location ?? "";
  return JSON.stringify(["cell", location, state.cell[0], state.cell[1]]);
}

function requirePositiveFinite(value, name) {
  if (!isFiniteNumber(value) || value <= 0) {
    throw new DesktopMapViewportError(
      "scale",
      `${name} must be a positive finite number`,
    );
  }
}

function isFiniteNumber(value) {
  return typeof value === "number" && Number.isFinite(value);
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}
