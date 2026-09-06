/**
 * DOM-independent coordinator for Vellum link taps and noun menus.
 *
 * Invariants:
 * - every dispatched link tap receives a unique, monotonically increasing id;
 * - only the newest pending request may publish a menu or dispatch a pick;
 * - picks use the command stored in the accepted server reply, never caller text;
 * - a pick is consumed before submission, so failure cannot cause an implicit retry;
 * - each GOALS browser route keeps one FIFO reservation (including a null
 *   tombstone), so an addressed URL reply can never navigate a newer tab;
 * - close invalidates pending/menu state without resetting request ids.
 */

const INTERNAL_COMMAND = /^(?:__|action:|menu:)/;
const URL_RESERVATION_TIMEOUT_MS = 15_000;

function isRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function freezeEffect(type, fields = {}) {
  return Object.freeze({ type, ...fields });
}

function menuRequestId(reply) {
  if (!isRecord(reply)) return null;
  const camel = reply.requestId;
  const wire = reply.request_id;
  if (camel !== undefined && wire !== undefined && camel !== wire) return null;
  const value = camel ?? wire;
  return Number.isSafeInteger(value) && value >= 1 ? value : null;
}

function normalizeLink(value) {
  if (
    !isRecord(value) ||
    typeof value.exist_id !== "string" ||
    value.exist_id.length === 0 ||
    typeof value.noun !== "string"
  ) {
    throw new DesktopInteractionError("link", "link activation is malformed");
  }
  return Object.freeze({
    exist_id: value.exist_id,
    noun: value.noun,
    text: typeof value.text === "string" ? value.text : "",
    coord: typeof value.coord === "string" && value.coord.length > 0
      ? value.coord
      : null,
  });
}

function normalizeMenu(reply) {
  const requestId = menuRequestId(reply);
  if (
    !isRecord(reply) ||
    requestId === null ||
    !Array.isArray(reply.items)
  ) {
    throw new DesktopInteractionError("menu", "menu reply is malformed");
  }
  const items = reply.items.map((item) => {
    if (
      !isRecord(item) ||
      typeof item.text !== "string" ||
      typeof item.command !== "string" ||
      typeof item.disabled !== "boolean"
    ) {
      throw new DesktopInteractionError("menu", "menu item is malformed");
    }
    return Object.freeze({
      text: item.text,
      command: item.command,
      disabled: item.disabled,
    });
  });
  return Object.freeze({
    requestId,
    noun: typeof reply.noun === "string" ? reply.noun : "",
    items: Object.freeze(items),
  });
}

function webUrl(link) {
  if (!/^https?:\/\//i.test(link.noun)) {
    throw new DesktopInteractionError("url", "game URL must use http or https");
  }
  return link.noun;
}

export class DesktopInteractionError extends Error {
  constructor(code, message, options = {}) {
    super(message);
    this.name = "DesktopInteractionError";
    this.code = code;
    if (options.cause !== undefined) this.cause = options.cause;
  }
}

/**
 * Coordinates semantic link activations through four observable operations.
 * Dependencies are injected adapters: `dispatch` accepts a DesktopSession
 * intent, `submit` accepts one server-provided command, `isOnline` reports
 * whether either remote operation is currently safe, and `openUrl` is optional.
 */
export class DesktopInteractionCoordinator {
  constructor({ dispatch, submit, isOnline, openUrl = null, reserveUrl = null } = {}) {
    if (typeof dispatch !== "function") throw new TypeError("dispatch must be a function");
    if (typeof submit !== "function") throw new TypeError("submit must be a function");
    if (typeof isOnline !== "function") throw new TypeError("isOnline must be a function");
    if (openUrl !== null && typeof openUrl !== "function") {
      throw new TypeError("openUrl must be a function or null");
    }
    if (reserveUrl !== null && typeof reserveUrl !== "function") {
      throw new TypeError("reserveUrl must be a function or null");
    }
    this._dispatch = dispatch;
    this._submit = submit;
    this._isOnline = isOnline;
    this._openUrl = openUrl;
    this._reserveUrl = reserveUrl;
    this._nextRequestId = 1;
    this._active = null;
    this._pendingUrls = [];
  }

  /** Submit a command, reserving a popup-safe tab only for GOALS browser routes. */
  submit(command) {
    const normalized = typeof command === "string" ? command.trim().toLowerCase() : "";
    const reservesUrl =
      normalized === "goals" || normalized === "goals web" || normalized === ".goals web";
    let target = null;
    if (reservesUrl && this._reserveUrl) {
      try {
        target = this._reserveUrl();
      } catch {
        target = null;
      }
    }

    let receipt;
    try {
      receipt = this._submit(command);
    } catch (error) {
      target?.close?.();
      throw error;
    }

    if (reservesUrl) {
      const pending = { target, timer: null };
      if (target) {
        pending.timer = setTimeout(() => {
          pending.timer = null;
          pending.target?.close?.();
          // Keep the queue slot as a tombstone. Removing it would let this
          // request's late reply navigate a newer request's reserved tab.
          pending.target = null;
        }, URL_RESERVATION_TIMEOUT_MS);
      }
      this._pendingUrls.push(pending);
    }
    return receipt;
  }

  /** Close and discard every reserved tab when their requests cannot complete. */
  cancelPendingUrls() {
    for (const pending of this._pendingUrls.splice(0)) {
      if (pending.timer !== null) clearTimeout(pending.timer);
      pending.target?.close?.();
    }
  }

  /** Navigate the oldest tab reserved for an addressed GOALS reply. */
  receiveOpenUrl(value) {
    const url = webUrl({ noun: value });
    const pending = this._pendingUrls.shift() ?? null;
    if (pending?.timer != null) clearTimeout(pending.timer);
    const target = pending?.target ?? null;
    if (target) {
      try {
        target.navigate(url);
      } catch (error) {
        target.close?.();
        throw new DesktopInteractionError("open-url", "game URL was not opened", {
          cause: error,
        });
      }
      return freezeEffect("url", { url, reserved: true });
    }
    return freezeEffect("url", { url, reserved: false, dropped: true });
  }

  /** Activate one protocol link and return a UI-facing effect. */
  activate(value) {
    const link = normalizeLink(value);
    if (link.exist_id === "_url_") {
      const url = webUrl(link);
      this.close();
      if (this._openUrl) {
        try {
          this._openUrl(url);
        } catch (error) {
          throw new DesktopInteractionError("open-url", "game URL was not opened", {
            cause: error,
          });
        }
      }
      return freezeEffect("url", { url });
    }

    if (!this._online()) {
      this.close();
      throw new DesktopInteractionError("offline", "the session is not connected");
    }
    if (this._nextRequestId > Number.MAX_SAFE_INTEGER) {
      throw new DesktopInteractionError("request-id", "link request ids are exhausted");
    }

    const requestId = this._nextRequestId++;
    const expectsMenu = link.exist_id !== "_direct_" && link.coord === null;
    let receipt;
    try {
      receipt = this._dispatch({ kind: "link-tap", requestId, link });
    } catch (error) {
      this.close();
      throw new DesktopInteractionError("dispatch", "link activation was not sent", {
        cause: error,
      });
    }

    this._active = expectsMenu
      ? Object.freeze({ phase: "pending", requestId, link, menu: null })
      : null;
    return freezeEffect(expectsMenu ? "pending-menu" : "dispatched", {
      requestId,
      expectsMenu,
      receipt,
    });
  }

  /** Accept only the menu reply for the newest pending noun request. */
  receiveMenu(reply) {
    const requestId = menuRequestId(reply);
    if (!this._active) {
      return freezeEffect("ignored-menu", { reason: "unknown", requestId });
    }
    if (requestId !== this._active.requestId) {
      return freezeEffect("ignored-menu", { reason: "stale", requestId });
    }

    let menu;
    try {
      menu = normalizeMenu(reply);
    } catch (error) {
      this.close();
      throw error;
    }
    this._active = Object.freeze({
      phase: "menu",
      requestId: menu.requestId,
      link: this._active.link,
      menu,
    });
    return freezeEffect("menu", { menu });
  }

  /**
   * Pick by correlated request id and item index. The caller cannot supply a
   * command, which keeps server menu data authoritative.
   */
  pick({ requestId, index } = {}) {
    if (
      !this._active ||
      this._active.phase !== "menu" ||
      requestId !== this._active.requestId
    ) {
      throw new DesktopInteractionError("stale-pick", "menu pick is stale or unknown");
    }
    if (!Number.isSafeInteger(index) || index < 0 || index >= this._active.menu.items.length) {
      throw new DesktopInteractionError("pick", "menu item index is invalid");
    }

    const item = this._active.menu.items[index];
    if (item.disabled) {
      throw new DesktopInteractionError("disabled", "menu item is disabled");
    }
    if (!item.command || INTERNAL_COMMAND.test(item.command)) {
      throw new DesktopInteractionError("command", "menu item has no dispatchable command");
    }
    if (!this._online()) {
      this.close();
      throw new DesktopInteractionError("offline", "the session is not connected");
    }

    // Consume before crossing the submit seam. If submission is uncertain,
    // this coordinator still cannot replay the command or accept a second pick.
    this._active = null;
    let receipt;
    try {
      receipt = this.submit(item.command);
    } catch (error) {
      throw new DesktopInteractionError("submit", "menu command was not sent", {
        cause: error,
      });
    }
    return freezeEffect("submitted", {
      requestId,
      index,
      label: item.text,
      receipt,
    });
  }

  /** Invalidate the current request/menu. Later activations remain usable. */
  close() {
    const requestId = this._active?.requestId ?? null;
    this._active = null;
    return freezeEffect("closed", { requestId });
  }

  _online() {
    try {
      return Boolean(this._isOnline());
    } catch {
      return false;
    }
  }
}

export default DesktopInteractionCoordinator;
