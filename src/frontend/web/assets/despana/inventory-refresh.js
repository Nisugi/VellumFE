export const DEFAULT_INVENTORY_REFRESH_TIMEOUT_MS = 35_000;

const IDLE_STATE = Object.freeze({
  kind: "idle",
  character: null,
  message: "",
});

function sameCharacter(left, right) {
  return String(left || "").trim().toLocaleLowerCase() ===
    String(right || "").trim().toLocaleLowerCase();
}

/** Track one visible `.invsync` request without coupling it to transport code. */
export class InventoryRefreshTracker {
  constructor({
    setTimeout = globalThis.setTimeout?.bind(globalThis),
    clearTimeout = globalThis.clearTimeout?.bind(globalThis),
    onChange = () => {},
    timeoutMs = DEFAULT_INVENTORY_REFRESH_TIMEOUT_MS,
  } = {}) {
    if (typeof setTimeout !== "function" || typeof clearTimeout !== "function") {
      throw new TypeError("Inventory refresh timers are required");
    }
    if (typeof onChange !== "function") {
      throw new TypeError("Inventory refresh onChange must be a function");
    }
    this._setTimeout = setTimeout;
    this._clearTimeout = clearTimeout;
    this._onChange = onChange;
    this._timeoutMs = timeoutMs;
    this._timer = null;
    this._generation = 0;
    this._state = IDLE_STATE;
  }

  get state() {
    return this._state;
  }

  begin(character) {
    const name = String(character || "").trim();
    if (!name) throw new TypeError("Inventory refresh character is required");
    this._cancelTimer();
    const generation = ++this._generation;
    this._publish("pending", name, "Refreshing nested inventory…");
    this._timer = this._setTimeout(() => {
      if (generation !== this._generation || this._state.kind !== "pending") return;
      this._timer = null;
      this._publish(
        "timed-out",
        name,
        "Inventory refresh timed out. Select Refresh to try again.",
      );
    }, this._timeoutMs);
  }

  receive(character, inventoryTree) {
    if (
      this._state.kind !== "pending" ||
      !sameCharacter(this._state.character, character)
    ) {
      return false;
    }
    const name = this._state.character;
    this._cancelTimer();
    if (!inventoryTree || typeof inventoryTree !== "object") {
      this._publish(
        "error",
        name,
        "Inventory refresh returned invalid data. Select Refresh to try again.",
      );
      return true;
    }
    if (inventoryTree.complete !== true) {
      this._publish(
        "incomplete",
        name,
        "Inventory refresh is incomplete. Select Refresh to try again.",
      );
      return true;
    }
    this._publish("ready", name, "Inventory refreshed.");
    return true;
  }

  fail(message) {
    if (this._state.kind !== "pending") return false;
    const name = this._state.character;
    this._cancelTimer();
    this._publish("error", name, String(message || "Inventory refresh failed."));
    return true;
  }

  reset() {
    this._cancelTimer();
    this._state = IDLE_STATE;
    this._onChange(this._state);
  }

  destroy() {
    this.reset();
  }

  _cancelTimer() {
    this._generation += 1;
    if (this._timer !== null) this._clearTimeout(this._timer);
    this._timer = null;
  }

  _publish(kind, character, message) {
    this._state = Object.freeze({ kind, character, message });
    this._onChange(this._state);
  }
}
