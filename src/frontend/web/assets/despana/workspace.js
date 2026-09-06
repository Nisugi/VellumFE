import {
  DEFAULT_DESPANA_LAYOUT,
  LAYOUT_ZONES,
  WorkspaceLayout,
  normalizeLayoutCharacter,
} from "./layout.js";

const MODULE_LABELS = Object.freeze({
  "active-spells": "Active Spells",
  "known-spells": "Known Spells",
  cooldowns: "Cooldowns",
  combat: "Combat",
  compass: "Compass",
  conditions: "Conditions / Roundtime",
  familiar: "Familiar",
  hands: "Hands / Prepared",
  injuries: "Injuries",
  inventory: "Inventory",
  map: "Map",
  room: "Room",
  story: "Story",
  thoughts: "Thoughts",
  tasks: "Tasks / Bounty / Society",
  vitals: "Vitals",
});

const ZONE_LABELS = Object.freeze({
  top: "Top",
  bottom: "Bottom",
  left: "Left",
  right: "Right",
  center: "Center",
});

const MIN_TRACK = 48;
const MIN_MIDDLE_WIDTH = 260;
const MIN_MIDDLE_HEIGHT = 200;
const MODULE_DRAG_THRESHOLD = 6;
const PAIR_KEYBOARD_STEP = 25;
const TRACK_KEYBOARD_STEP = 12;

/**
 * Coordinates module rendering and the browser-only Despana workspace.
 *
 * Vellum state never enters this class directly: renderers receive the
 * normalized DesktopSession view, while every placement change crosses the
 * pure WorkspaceLayout intent seam.
 */
export class DesktopWorkspace {
  constructor(root, options = {}) {
    if (!root || typeof root.querySelectorAll !== "function") {
      throw new TypeError("DesktopWorkspace root must support querySelectorAll");
    }

    this.root = root;
    this.document = root.ownerDocument;
    this.window = this.document.defaultView || globalThis;
    this.storage = options.storage || null;
    this.reportError = typeof options.reportError === "function"
      ? options.reportError
      : () => {};
    this.defaults = options.defaults || DEFAULT_DESPANA_LAYOUT;
    this.modules = new Map();
    this.destroyed = false;
    this.character = null;
    this.normalizedCharacter = null;
    this._drop = null;
    this._modulePointerDrag = null;
    this._activeModuleDragCleanup = null;
    this._activeResizeCleanup = null;
    this._openModuleButton = null;
    this._resizeFrame = null;
    this._layoutRevision = 0;
    this._loadGeneration = 0;
    this._listeners = new AbortController();

    this.workspaceElement = root.querySelector("#workspace");
    this.middleElement = root.querySelector(".workspace-middle");
    this.workspaceStatus = root.querySelector("#workspace-status");
    this.workspaceMenuButton = root.querySelector("#workspace-menu-button");
    this.workspaceMenu = root.querySelector("#workspace-menu");
    if (
      !this.workspaceElement ||
      !this.middleElement ||
      !this.workspaceStatus ||
      !this.workspaceMenuButton ||
      !this.workspaceMenu
    ) {
      throw new Error("Vellum Despana workspace shell is incomplete");
    }

    this.zones = new Map();
    for (const element of root.querySelectorAll("[data-zone]")) {
      const zone = element.getAttribute("data-zone");
      if (!LAYOUT_ZONES.includes(zone) || this.zones.has(zone)) {
        throw new Error(`invalid or duplicate workspace zone: ${zone}`);
      }
      this.zones.set(zone, element);
    }
    if (this.zones.size !== LAYOUT_ZONES.length) {
      throw new Error("Vellum Despana workspace must expose exactly five zones");
    }

    this.moduleElements = new Map();
    for (const element of root.querySelectorAll("[data-module]")) {
      const id = element.getAttribute("data-module");
      if (!id || this.moduleElements.has(id)) {
        throw new Error(`invalid or duplicate workspace module: ${id}`);
      }
      this.moduleElements.set(id, element);
    }
    if (!this.moduleElements.size) throw new Error("Vellum Despana workspace has no modules");

    this.hiddenDepot = this.document.createElement("div");
    this.hiddenDepot.id = "workspace-hidden-modules";
    this.hiddenDepot.hidden = true;
    root.appendChild(this.hiddenDepot);

    this.moduleMenu = this.document.createElement("div");
    this.moduleMenu.id = "module-menu";
    this.moduleMenu.className = "module-menu module-menu-floating";
    this.moduleMenu.setAttribute("role", "menu");
    this.moduleMenu.hidden = true;
    this.document.body.appendChild(this.moduleMenu);

    this.layout = WorkspaceLayout.restore({
      moduleIds: this.moduleElements.keys(),
      defaults: this.defaults,
      character: "pending",
    });

    this.#installModuleControls();
    this.#installTrackSeparators();
    this.#installEvents();
    this.#reconcile();
  }

  register({ id, slices, render } = {}) {
    this.#assertActive();
    if (typeof id !== "string" || id.length === 0 || id.trim() !== id) {
      throw new TypeError("module id must be a non-empty, trimmed string");
    }
    if (this.modules.has(id)) throw new Error(`module already registered: ${id}`);
    if (!Array.isArray(slices) || slices.length === 0) {
      throw new TypeError(`module ${id} must declare at least one state slice`);
    }
    if (slices.some((slice) => typeof slice !== "string" || slice.length === 0)) {
      throw new TypeError(`module ${id} has an invalid state slice`);
    }
    if (typeof render !== "function") {
      throw new TypeError(`module ${id} render must be a function`);
    }

    const module = this.moduleElements.get(id);
    if (!module) throw new Error(`module ${id} must match one [data-module] element`);
    const body = module.querySelector("[data-module-body]");
    if (!body) throw new Error(`module ${id} is missing [data-module-body]`);

    const context = Object.freeze({ id, module, body });
    this.modules.set(id, { slices: new Set(slices), render, context });
    return () => this.unregister(id);
  }

  unregister(id) {
    if (this.destroyed) return false;
    return this.modules.delete(id);
  }

  /** Load one character's layout. Repeated calls for the same identity are no-ops. */
  setCharacter(character) {
    this.#assertActive();
    if (typeof character !== "string" || !character.trim()) return false;
    let normalized;
    try {
      normalized = normalizeLayoutCharacter(character);
    } catch (error) {
      this.#surfaceError(error);
      return false;
    }
    if (normalized === this.normalizedCharacter) return false;

    let saved = null;
    if (this.storage) {
      try {
        saved = this.storage.read(character);
      } catch (error) {
        this.#surfaceError(error, "Unable to read the saved workspace");
      }
    }
    this.character = character.trim();
    this.normalizedCharacter = normalized;
    this._layoutRevision = 0;
    const generation = ++this._loadGeneration;
    this.layout = WorkspaceLayout.restore({
      moduleIds: this.moduleElements.keys(),
      defaults: this.defaults,
      saved,
      character,
    });
    this.#reconcile();
    this.#setStatus(saved ? `${this.character} workspace restored` : "Default workspace");
    this.#loadSharedWorkspace(generation, saved);
    return true;
  }

  async #loadSharedWorkspace(generation, localSaved) {
    if (!this.storage || typeof this.storage.load !== "function" || !this.character) return;
    const character = this.character;
    const normalized = this.normalizedCharacter;
    const revision = this._layoutRevision;
    try {
      const shared = await this.storage.load(character);
      if (
        this.destroyed ||
        generation !== this._loadGeneration ||
        normalized !== this.normalizedCharacter ||
        revision !== this._layoutRevision
      ) {
        return;
      }
      if (!shared) {
        // Migrate only the model's validated, canonical snapshot. The local
        // value may come from an older build or have been partially written.
        if (localSaved) await this.storage.write(character, this.layout.serialize());
        return;
      }

      const restored = WorkspaceLayout.restore({
        moduleIds: this.moduleElements.keys(),
        defaults: this.defaults,
        saved: shared,
        character,
      });
      const canonical = restored.serialize();
      this.layout = restored;
      this.storage.cache?.(character, canonical);
      this.#reconcile();
      this.#setStatus(`${this.character} workspace restored`);
      if (canonical !== shared) await this.storage.write(character, canonical);
    } catch (error) {
      this.#surfaceError(error, "Unable to synchronize the saved workspace");
    }
  }

  render(view, changedSlices = null) {
    this.#assertActive();
    const changed = changedSlices == null ? null : this.#sliceSet(changedSlices);
    for (const entry of this.modules.values()) {
      if (changed && !this.#isAffected(entry.slices, changed)) continue;
      const { module, id } = entry.context;
      try {
        entry.render(view, entry.context);
        module.classList.remove("module-error");
        module.removeAttribute("data-module-error");
        module.querySelector(":scope > .module-error-message")?.remove();
      } catch (error) {
        const detail = error instanceof Error && error.message ? ` ${error.message}` : "";
        module.classList.add("module-error");
        module.setAttribute("data-module-error", "true");
        let fallback = module.querySelector(":scope > .module-error-message");
        if (!fallback) {
          fallback = this.document.createElement("div");
          fallback.className = "module-error-message";
          fallback.setAttribute("role", "alert");
          module.appendChild(fallback);
        }
        fallback.textContent = `Unable to render ${id}.${detail}`;
      }
    }
  }

  destroy() {
    if (this.destroyed) return;
    this._activeModuleDragCleanup?.();
    this._activeResizeCleanup?.();
    if (this._resizeFrame !== null) {
      const cancel = this.window.cancelAnimationFrame || this.window.clearTimeout;
      cancel?.call(this.window, this._resizeFrame);
      this._resizeFrame = null;
    }
    this._listeners.abort();
    this._loadGeneration += 1;
    this.moduleMenu.remove();
    this.modules.clear();
    this.moduleElements.clear();
    this.root = null;
    this.destroyed = true;
  }

  #installModuleControls() {
    for (const [id, module] of this.moduleElements) {
      module.draggable = false;
      const handle = module.querySelector(":scope > .pane-header") ||
        module.querySelector(":scope > h2") || module;
      handle.draggable = false;
      handle.setAttribute("data-module-drag-handle", "true");
      handle.title = `Drag ${this.#label(id)} to reposition`;

      const button = this.document.createElement("button");
      button.type = "button";
      button.className = "module-menu-button";
      button.dataset.moduleMenu = id;
      button.setAttribute("aria-label", `Move or hide ${this.#label(id)}`);
      button.setAttribute("aria-haspopup", "menu");
      button.setAttribute("aria-expanded", "false");
      button.textContent = "⋮";
      module.appendChild(button);
    }
  }

  #installTrackSeparators() {
    for (const [zone, parent, orientation] of [
      ["top", this.workspaceElement, "horizontal"],
      ["bottom", this.workspaceElement, "horizontal"],
      ["left", this.middleElement, "vertical"],
      ["right", this.middleElement, "vertical"],
    ]) {
      const separator = this.document.createElement("div");
      separator.className = `track-separator track-separator-${zone}`;
      separator.dataset.trackZone = zone;
      separator.setAttribute("role", "separator");
      separator.setAttribute("tabindex", "0");
      separator.setAttribute("aria-orientation", orientation);
      separator.setAttribute("aria-label", `Resize ${ZONE_LABELS[zone]} zone`);
      separator.setAttribute("aria-valuemin", String(MIN_TRACK));
      parent.appendChild(separator);
    }
  }

  #installEvents() {
    const signal = this._listeners.signal;
    this.document.addEventListener("click", (event) => this.#handleClick(event), { signal });
    this.document.addEventListener("keydown", (event) => this.#handleKeydown(event), { signal });
    this.root.addEventListener("pointerdown", (event) => this.#handlePointerDown(event), { signal });
    this.window.addEventListener("resize", () => this.#scheduleTrackSync(), { signal });
  }

  #handleClick(event) {
    const target = event.target instanceof this.window.Element ? event.target : null;
    if (!target) return;
    const action = target.closest("[data-layout-action]");
    if (action) {
      event.preventDefault();
      this.#runMenuAction(action);
      return;
    }
    const moduleButton = target.closest("[data-module-menu]");
    if (moduleButton) {
      event.preventDefault();
      const id = moduleButton.dataset.moduleMenu;
      if (this._openModuleButton === moduleButton && !this.moduleMenu.hidden) {
        this.#closeMenus(true);
      } else {
        this.#openModuleMenu(id, moduleButton);
      }
      return;
    }
    if (target.closest("#workspace-menu-button")) {
      event.preventDefault();
      this.#toggleWorkspaceMenu();
      return;
    }
    if (!target.closest(".module-menu")) this.#closeMenus(false);
  }

  #handleKeydown(event) {
    if (event.key === "Escape" && this._modulePointerDrag) {
      event.preventDefault();
      this.#cancelModulePointerDrag();
      return;
    }
    if (event.key === "Escape" && (!this.moduleMenu.hidden || !this.workspaceMenu.hidden)) {
      event.preventDefault();
      this.#closeMenus(true);
      return;
    }
    const target = event.target instanceof this.window.Element ? event.target : null;
    const menu = target?.closest('[role="menu"]');
    if (menu && ["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      const items = [...menu.querySelectorAll('[role="menuitem"]:not(:disabled)')];
      if (!items.length) return;
      event.preventDefault();
      const current = items.indexOf(this.document.activeElement);
      const index = event.key === "Home" ? 0
        : event.key === "End" ? items.length - 1
          : event.key === "ArrowDown" ? (current + 1 + items.length) % items.length
            : (current - 1 + items.length) % items.length;
      items[index].focus();
      return;
    }
    if (menu && event.key === "Tab") {
      this.window.setTimeout(() => this.#closeMenus(false), 0);
      return;
    }
    const separator = target?.closest('[role="separator"]');
    if (!separator) return;
    if (separator.dataset.trackZone) this.#resizeTrackWithKeyboard(separator, event);
    else if (separator.dataset.resizePair) this.#resizePairWithKeyboard(separator, event);
  }

  #runMenuAction(element) {
    const action = element.dataset.layoutAction;
    const id = element.dataset.moduleId;
    const snapshot = this.layout.snapshot();
    try {
      switch (action) {
        case "move-zone": {
          const zone = element.dataset.zone;
          this.#apply(
            { type: "move", id, zone, index: snapshot.zones[zone].modules.length },
            `${this.#label(id)} moved to ${ZONE_LABELS[zone]}`,
          );
          break;
        }
        case "move-earlier":
        case "move-later": {
          const location = this.#location(id);
          if (!location || location.kind !== "zone") break;
          const delta = action === "move-earlier" ? -1 : 1;
          this.#apply(
            { type: "move", id, zone: location.zone, index: location.index + delta },
            `${this.#label(id)} reordered`,
          );
          break;
        }
        case "hide":
          this.#apply({ type: "hide", id }, `${this.#label(id)} hidden`);
          break;
        case "show":
          this.#apply({ type: "show", id }, `${this.#label(id)} restored`);
          break;
        case "set-flow": {
          const zone = element.dataset.zone;
          const flow = element.dataset.flow;
          this.#apply(
            { type: "set-flow", zone, flow },
            `${ZONE_LABELS[zone]} zone split ${flow === "horizontal" ? "left / right" : "top / bottom"}`,
          );
          break;
        }
        case "reset":
          this.#apply({ type: "reset" }, "Default workspace restored");
          break;
        default:
          return;
      }
    } catch (error) {
      this.#surfaceError(error);
    } finally {
      this.#closeMenus(true);
    }
  }

  #openModuleMenu(id, button) {
    const location = this.#location(id);
    if (!location || location.kind !== "zone") return;
    this.#closeMenus(false);
    this._openModuleButton = button;
    button.setAttribute("aria-expanded", "true");
    this.moduleMenu.replaceChildren();
    this.#appendMenuHeading(this.moduleMenu, this.#label(id));
    for (const zone of ["top", "bottom", "left", "right", "center"]) {
      this.moduleMenu.appendChild(this.#menuItem(`Move to ${ZONE_LABELS[zone]}`, {
        layoutAction: "move-zone", moduleId: id, zone,
      }));
    }
    this.moduleMenu.appendChild(this.#menuItem("Move earlier", {
      layoutAction: "move-earlier", moduleId: id,
    }, location.index === 0));
    this.moduleMenu.appendChild(this.#menuItem("Move later", {
      layoutAction: "move-later", moduleId: id,
    }, location.index === this.layout.snapshot().zones[location.zone].modules.length - 1));
    this.#appendMenuHeading(this.moduleMenu, `${ZONE_LABELS[location.zone]} zone layout`);
    this.moduleMenu.appendChild(this.#menuItem("Split left / right", {
      layoutAction: "set-flow", moduleId: id, zone: location.zone, flow: "horizontal",
    }));
    this.moduleMenu.appendChild(this.#menuItem("Split top / bottom", {
      layoutAction: "set-flow", moduleId: id, zone: location.zone, flow: "vertical",
    }));
    this.moduleMenu.appendChild(this.#menuItem(`Hide ${this.#label(id)}`, {
      layoutAction: "hide", moduleId: id,
    }));

    const rect = button.getBoundingClientRect();
    this.moduleMenu.hidden = false;
    const width = this.moduleMenu.offsetWidth || 220;
    const height = this.moduleMenu.offsetHeight || 320;
    const left = Math.max(8, Math.min(rect.right - width, this.window.innerWidth - width - 8));
    const top = Math.max(8, Math.min(rect.bottom + 4, this.window.innerHeight - height - 8));
    this.moduleMenu.style.left = `${left}px`;
    this.moduleMenu.style.top = `${top}px`;
    this.moduleMenu.querySelector('[role="menuitem"]:not(:disabled)')?.focus();
  }

  #toggleWorkspaceMenu() {
    if (!this.workspaceMenu.hidden) {
      this.#closeMenus(true);
      return;
    }
    this.#closeMenus(false);
    this.#renderWorkspaceMenu();
    this.workspaceMenu.hidden = false;
    this.workspaceMenuButton.setAttribute("aria-expanded", "true");
    this.workspaceMenu.querySelector('[role="menuitem"]:not(:disabled)')?.focus();
  }

  #renderWorkspaceMenu() {
    this.workspaceMenu.replaceChildren();
    const hidden = this.layout.snapshot().hidden;
    this.#appendMenuHeading(this.workspaceMenu, "Hidden modules");
    if (!hidden.length) {
      this.workspaceMenu.appendChild(this.#menuItem("No hidden modules", {}, true));
    } else {
      for (const entry of hidden) {
        this.workspaceMenu.appendChild(this.#menuItem(`Show ${this.#label(entry.id)}`, {
          layoutAction: "show", moduleId: entry.id,
        }));
      }
    }
    this.#appendMenuHeading(this.workspaceMenu, "Workspace");
    this.workspaceMenu.appendChild(this.#menuItem("Restore default layout", {
      layoutAction: "reset",
    }));
  }

  #menuItem(label, data, disabled = false) {
    const button = this.document.createElement("button");
    button.type = "button";
    button.setAttribute("role", "menuitem");
    button.textContent = label;
    button.disabled = disabled;
    for (const [key, value] of Object.entries(data)) button.dataset[key] = value;
    return button;
  }

  #appendMenuHeading(menu, text) {
    const heading = this.document.createElement("div");
    heading.className = "module-menu-heading";
    heading.setAttribute("role", "presentation");
    heading.textContent = text;
    menu.appendChild(heading);
  }

  #closeMenus(restoreFocus) {
    const moduleButton = this._openModuleButton;
    this.moduleMenu.hidden = true;
    this.moduleMenu.replaceChildren();
    this.moduleMenu.style.removeProperty("left");
    this.moduleMenu.style.removeProperty("top");
    moduleButton?.setAttribute("aria-expanded", "false");
    this._openModuleButton = null;
    const workspaceWasOpen = !this.workspaceMenu.hidden;
    this.workspaceMenu.hidden = true;
    this.workspaceMenuButton.setAttribute("aria-expanded", "false");
    if (!restoreFocus) return;
    if (moduleButton?.isConnected && !moduleButton.closest("[hidden]")) moduleButton.focus();
    else if (workspaceWasOpen) this.workspaceMenuButton.focus();
    else this.workspaceMenuButton.focus();
  }

  #handlePointerDown(event) {
    if (event.button !== 0 || event.isPrimary === false) return;
    const target = event.target instanceof this.window.Element ? event.target : null;
    if (!target) return;
    const separator = target.closest('[role="separator"]');
    if (separator) {
      event.preventDefault();
      if (separator.dataset.trackZone) this.#beginTrackResize(separator, event);
      else if (separator.dataset.resizePair) this.#beginPairResize(separator, event);
      return;
    }
    this.#beginModulePointerDrag(target, event);
  }

  #beginModulePointerDrag(target, event) {
    const handle = target.closest("[data-module-drag-handle]");
    const module = handle?.closest("[data-module]");
    if (
      !module ||
      target.closest('button, input, textarea, select, a, [role="link"]') ||
      this._modulePointerDrag ||
      this._activeResizeCleanup
    ) return;

    event.preventDefault();
    this.#closeMenus(false);
    const drag = {
      active: false,
      controller: new AbortController(),
      handle,
      id: module.dataset.module,
      module,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
    };
    this._modulePointerDrag = drag;
    const cleanup = () => {
      drag.controller.abort();
      try {
        if (drag.handle.hasPointerCapture?.(drag.pointerId)) {
          drag.handle.releasePointerCapture(drag.pointerId);
        }
      } catch {
        // Pointer capture is best-effort; window listeners still finish cleanup.
      }
      drag.module.classList.remove("is-dragging");
      this.#clearDropTarget();
      if (this._modulePointerDrag === drag) this._modulePointerDrag = null;
      if (this._activeModuleDragCleanup === cleanup) this._activeModuleDragCleanup = null;
    };
    this._activeModuleDragCleanup = cleanup;

    try {
      handle.setPointerCapture?.(drag.pointerId);
    } catch {
      // Some embedded browsers lack pointer capture; window listeners are enough.
    }
    this.window.addEventListener("pointermove", (move) => {
      if (move.pointerId !== drag.pointerId) return;
      if (!drag.active) {
        const distance = Math.hypot(move.clientX - drag.startX, move.clientY - drag.startY);
        if (distance < MODULE_DRAG_THRESHOLD) return;
        drag.active = true;
        drag.module.classList.add("is-dragging");
      }
      move.preventDefault();
      this.#updateModuleDropTargetAt(move.clientX, move.clientY, drag.id);
    }, { signal: drag.controller.signal });
    this.window.addEventListener("pointerup", (up) => {
      if (up.pointerId !== drag.pointerId) return;
      if (drag.active) {
        up.preventDefault();
        this.#updateModuleDropTargetAt(up.clientX, up.clientY, drag.id);
      }
      const drop = drag.active && this._drop ? { ...this._drop } : null;
      cleanup();
      if (!drop) return;
      try {
        this.#apply(
          { type: "move", id: drag.id, zone: drop.zone, index: drop.index },
          `${this.#label(drag.id)} moved to ${ZONE_LABELS[drop.zone]}`,
        );
      } catch (error) {
        this.#surfaceError(error);
      }
    }, { signal: drag.controller.signal });
    this.window.addEventListener("pointercancel", (cancel) => {
      if (cancel.pointerId !== drag.pointerId) return;
      cleanup();
    }, { signal: drag.controller.signal });
  }

  #updateModuleDropTargetAt(clientX, clientY, draggedId) {
    const target = this.document.elementFromPoint?.(clientX, clientY);
    if (!(target instanceof this.window.Element)) {
      this.#clearDropTarget();
      return;
    }
    const zoneElement = target.closest("[data-zone]");
    if (!zoneElement) {
      this.#clearDropTarget();
      return;
    }
    const zone = zoneElement.dataset.zone;
    const snapshot = this.layout.snapshot();
    const entries = snapshot.zones[zone].modules;
    let index = entries.length;
    let targetModule = target.closest("[data-module]");
    let position = "after";
    const pairSeparator = target.closest("[data-resize-pair]");
    if (pairSeparator) {
      index = entries.findIndex((entry) => entry.id === pairSeparator.dataset.after);
      targetModule = null;
      position = "before";
    } else if (targetModule) {
      const targetIndex = entries.findIndex((entry) => entry.id === targetModule.dataset.module);
      if (targetIndex >= 0) {
        const rect = targetModule.getBoundingClientRect();
        const horizontal = snapshot.zones[zone].flow === "horizontal";
        const coordinate = horizontal ? clientX : clientY;
        const midpoint = horizontal ? rect.left + rect.width / 2 : rect.top + rect.height / 2;
        position = coordinate < midpoint ? "before" : "after";
        index = targetIndex + (position === "after" ? 1 : 0);
      }
    }
    const source = this.#location(draggedId);
    if (source?.kind === "zone" && source.zone === zone && source.index < index) index -= 1;
    index = Math.max(0, index);
    this.#clearDropTarget();
    this._drop = { zone, index };
    zoneElement.dataset.dropTarget = "true";
    if (targetModule) targetModule.dataset.dropPosition = position;
  }

  #clearDropTarget() {
    for (const zone of this.zones.values()) zone.removeAttribute("data-drop-target");
    for (const module of this.moduleElements.values()) module.removeAttribute("data-drop-position");
    this._drop = null;
  }

  #cancelModulePointerDrag() {
    this._activeModuleDragCleanup?.();
  }

  #beginTrackResize(separator, event) {
    const zone = separator.dataset.trackZone;
    const horizontalAxis = zone === "left" || zone === "right";
    const sign = zone === "right" || zone === "bottom" ? -1 : 1;
    const startCoordinate = horizontalAxis ? event.clientX : event.clientY;
    const startPixels = this.#renderedTracks()[zone];
    let preview = startPixels;
    this.#beginPointerResize(separator, event.pointerId, (move) => {
      const coordinate = horizontalAxis ? move.clientX : move.clientY;
      preview = this.#clampTrackForViewport(zone, startPixels + (coordinate - startCoordinate) * sign);
      this.workspaceElement.style.setProperty(`--workspace-${zone}`, `${preview}px`);
      separator.setAttribute("aria-valuenow", String(Math.round(preview)));
    }, () => this.#apply(
      { type: "resize-track", zone, pixels: preview },
      `${ZONE_LABELS[zone]} zone resized`,
    ));
  }

  #beginPairResize(separator, event) {
    const zone = separator.dataset.zone;
    const before = separator.dataset.before;
    const after = separator.dataset.after;
    const zoneState = this.layout.snapshot().zones[zone];
    const first = zoneState.modules.find((entry) => entry.id === before);
    const second = zoneState.modules.find((entry) => entry.id === after);
    if (!first || !second) return;
    const horizontal = zoneState.flow === "horizontal";
    const startCoordinate = horizontal ? event.clientX : event.clientY;
    const firstElement = this.moduleElements.get(before);
    const secondElement = this.moduleElements.get(after);
    const firstRect = firstElement.getBoundingClientRect();
    const secondRect = secondElement.getBoundingClientRect();
    const pixels = Math.max(1, horizontal
      ? firstRect.width + secondRect.width
      : firstRect.height + secondRect.height);
    const pairTotal = first.weight + second.weight;
    let weightDelta = 0;
    this.#beginPointerResize(separator, event.pointerId, (move) => {
      const coordinate = horizontal ? move.clientX : move.clientY;
      weightDelta = Math.round(((coordinate - startCoordinate) / pixels) * pairTotal);
      const nextFirst = Math.max(1, Math.min(pairTotal - 1, first.weight + weightDelta));
      weightDelta = nextFirst - first.weight;
      firstElement.style.flexGrow = String(nextFirst);
      secondElement.style.flexGrow = String(pairTotal - nextFirst);
      separator.setAttribute("aria-valuenow", String(nextFirst));
    }, () => this.#apply(
      { type: "resize-pair", zone, before, after, delta: weightDelta },
      `${this.#label(before)} and ${this.#label(after)} resized`,
    ));
  }

  #beginPointerResize(separator, pointerId, move, finish) {
    this._activeResizeCleanup?.();
    separator.classList.add("is-resizing");
    separator.setPointerCapture?.(pointerId);
    const controller = new AbortController();
    const cleanup = () => {
      controller.abort();
      separator.classList.remove("is-resizing");
      this._activeResizeCleanup = null;
    };
    this._activeResizeCleanup = cleanup;
    this.window.addEventListener("pointermove", move, { signal: controller.signal });
    this.window.addEventListener("pointerup", () => {
      cleanup();
      try {
        finish();
      } catch (error) {
        this.#surfaceError(error);
        this.#reconcile();
      }
    }, { once: true, signal: controller.signal });
    this.window.addEventListener("pointercancel", () => {
      cleanup();
      this.#reconcile();
    }, { once: true, signal: controller.signal });
  }

  #resizeTrackWithKeyboard(separator, event) {
    const zone = separator.dataset.trackZone;
    const keyDelta = {
      left: { ArrowLeft: -1, ArrowRight: 1 },
      right: { ArrowLeft: 1, ArrowRight: -1 },
      top: { ArrowUp: -1, ArrowDown: 1 },
      bottom: { ArrowUp: 1, ArrowDown: -1 },
    }[zone]?.[event.key];
    if (!keyDelta) return;
    event.preventDefault();
    const pixels = this.#clampTrackForViewport(
      zone,
      this.#renderedTracks()[zone] + keyDelta * TRACK_KEYBOARD_STEP,
    );
    this.#apply(
      { type: "resize-track", zone, pixels },
      `${ZONE_LABELS[zone]} zone resized`,
    );
  }

  #resizePairWithKeyboard(separator, event) {
    const zone = separator.dataset.zone;
    const before = separator.dataset.before;
    const after = separator.dataset.after;
    const horizontal = this.layout.snapshot().zones[zone].flow === "horizontal";
    const delta = horizontal
      ? { ArrowLeft: -PAIR_KEYBOARD_STEP, ArrowRight: PAIR_KEYBOARD_STEP }[event.key]
      : { ArrowUp: -PAIR_KEYBOARD_STEP, ArrowDown: PAIR_KEYBOARD_STEP }[event.key];
    if (!delta) return;
    event.preventDefault();
    this.#apply({
      type: "resize-pair",
      zone,
      before,
      after,
      delta,
    }, `${ZONE_LABELS[zone]} modules resized`);
    [...this.zones.get(zone).querySelectorAll("[data-resize-pair]")]
      .find((candidate) => candidate.dataset.before === before && candidate.dataset.after === after)
      ?.focus();
  }

  #apply(intent, message) {
    this.layout.apply(intent);
    this.#reconcile();
    this.#persist();
    this.#setStatus(message);
  }

  #reconcile() {
    const snapshot = this.layout.snapshot();
    const scrollPositions = new Map(
      [...this.root.querySelectorAll(".text-output")]
        .map((element) => [element, [element.scrollLeft, element.scrollTop]]),
    );
    for (const zone of LAYOUT_ZONES) {
      const element = this.zones.get(zone);
      const zoneState = snapshot.zones[zone];
      element.dataset.zoneFlow = zoneState.flow;
      for (const separator of element.querySelectorAll(":scope > [data-resize-pair]")) {
        separator.remove();
      }
      zoneState.modules.forEach((entry, index) => {
        const module = this.moduleElements.get(entry.id);
        module.hidden = false;
        module.style.flexGrow = String(entry.weight);
        module.style.flexShrink = "1";
        module.style.flexBasis = "0px";
        element.appendChild(module);
        if (index < zoneState.modules.length - 1) {
          element.appendChild(this.#pairSeparator(
            zone,
            entry.id,
            zoneState.modules[index + 1].id,
            zoneState.flow,
            entry.weight,
            entry.weight + zoneState.modules[index + 1].weight,
          ));
        }
      });
    }
    for (const entry of snapshot.hidden) {
      const module = this.moduleElements.get(entry.id);
      module.hidden = true;
      this.hiddenDepot.appendChild(module);
    }
    this.#syncRenderedTracks();
    for (const [element, [left, top]] of scrollPositions) {
      element.scrollLeft = left;
      element.scrollTop = top;
    }
    if (!this.workspaceMenu.hidden) this.#renderWorkspaceMenu();
  }

  #pairSeparator(zone, before, after, flow, current, total) {
    const separator = this.document.createElement("div");
    separator.className = "workspace-separator";
    separator.dataset.resizePair = `${before}:${after}`;
    separator.dataset.zone = zone;
    separator.dataset.before = before;
    separator.dataset.after = after;
    separator.setAttribute("role", "separator");
    separator.setAttribute("tabindex", "0");
    separator.setAttribute("aria-orientation", flow === "horizontal" ? "vertical" : "horizontal");
    separator.setAttribute("aria-label", `Resize ${this.#label(before)} and ${this.#label(after)}`);
    separator.setAttribute("aria-valuemin", "1");
    separator.setAttribute("aria-valuemax", String(total - 1));
    separator.setAttribute("aria-valuenow", String(current));
    return separator;
  }

  #renderedTracks() {
    const raw = this.layout.snapshot().tracks;
    const width = this.middleElement.clientWidth || this.workspaceElement.clientWidth || 1200;
    const height = this.workspaceElement.clientHeight || 800;
    const horizontal = this.#fitTrackPair(raw.left, raw.right, width - MIN_MIDDLE_WIDTH);
    const vertical = this.#fitTrackPair(raw.top, raw.bottom, height - MIN_MIDDLE_HEIGHT);
    return { left: horizontal.first, right: horizontal.second,
      top: vertical.first, bottom: vertical.second };
  }

  #syncRenderedTracks() {
    const tracks = this.#renderedTracks();
    for (const [zone, pixels] of Object.entries(tracks)) {
      this.workspaceElement.style.setProperty(`--workspace-${zone}`, `${pixels}px`);
      const separator = this.root.querySelector(`[data-track-zone="${zone}"]`);
      separator?.setAttribute("aria-valuenow", String(Math.round(pixels)));
      separator?.setAttribute("aria-valuemax", String(this.#trackMaximum(zone, tracks)));
    }
  }

  #scheduleTrackSync() {
    if (this.destroyed || this._resizeFrame !== null) return;
    const schedule = this.window.requestAnimationFrame || this.window.setTimeout;
    this._resizeFrame = schedule.call(this.window, () => {
      this._resizeFrame = null;
      if (!this.destroyed) this.#syncRenderedTracks();
    });
  }

  #trackMaximum(zone, tracks = this.#renderedTracks()) {
    const horizontal = zone === "left" || zone === "right";
    const total = horizontal ? (this.middleElement.clientWidth || 1200)
      : (this.workspaceElement.clientHeight || 800);
    const minimumMiddle = horizontal ? MIN_MIDDLE_WIDTH : MIN_MIDDLE_HEIGHT;
    const opposite = { left: "right", right: "left", top: "bottom", bottom: "top" }[zone];
    return Math.max(MIN_TRACK, Math.round(total - tracks[opposite] - minimumMiddle));
  }

  #fitTrackPair(first, second, available) {
    const room = Math.max(MIN_TRACK * 2, available);
    let a = Math.max(MIN_TRACK, first);
    let b = Math.max(MIN_TRACK, second);
    if (a + b <= room) return { first: a, second: b };
    const scale = (room - MIN_TRACK * 2) / Math.max(1, a + b - MIN_TRACK * 2);
    a = MIN_TRACK + (a - MIN_TRACK) * Math.max(0, scale);
    b = MIN_TRACK + (b - MIN_TRACK) * Math.max(0, scale);
    return { first: Math.round(a), second: Math.round(b) };
  }

  #clampTrackForViewport(zone, value) {
    const tracks = this.#renderedTracks();
    const maximum = this.#trackMaximum(zone, tracks);
    return Math.max(MIN_TRACK, Math.min(maximum, Math.round(value)));
  }

  #persist() {
    if (!this.storage || !this.character) return;
    const revision = ++this._layoutRevision;
    try {
      const pending = this.storage.write(this.character, this.layout.serialize());
      if (pending && typeof pending.then === "function") {
        pending.then((result) => {
          if (!result?.superseded || revision !== this._layoutRevision) return;
          this.layout = WorkspaceLayout.restore({
            moduleIds: this.moduleElements.keys(),
            defaults: this.defaults,
            saved: result.layout,
            character: this.character,
          });
          this.#reconcile();
          this.#setStatus(`${this.character} workspace updated from another session`);
        }, (error) => {
          this.#surfaceError(error, "Workspace changed but could not be saved");
        });
      }
    } catch (error) {
      this.#surfaceError(error, "Workspace changed but could not be saved");
    }
  }

  flush() {
    if (!this.storage || !this.character || typeof this.storage.flush !== "function") return null;
    try {
      const pending = this.storage.flush(this.character);
      pending?.catch?.((error) => {
        this.#surfaceError(error, "Workspace changed but could not be saved");
      });
      return pending;
    } catch (error) {
      this.#surfaceError(error, "Workspace changed but could not be saved");
      return null;
    }
  }

  #location(id) {
    const snapshot = this.layout.snapshot();
    for (const zone of LAYOUT_ZONES) {
      const index = snapshot.zones[zone].modules.findIndex((entry) => entry.id === id);
      if (index >= 0) return { kind: "zone", zone, index };
    }
    const index = snapshot.hidden.findIndex((entry) => entry.id === id);
    return index >= 0 ? { kind: "hidden", index } : null;
  }

  #label(id) {
    return MODULE_LABELS[id] || id.split("-")
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1)).join(" ");
  }

  #setStatus(message) {
    this.workspaceStatus.textContent = message;
  }

  #surfaceError(error, message = null) {
    const detail = error instanceof Error ? error.message : String(error);
    this.#setStatus(message ? `${message}: ${detail}` : detail);
    try {
      this.reportError(error);
    } catch {
      // Workspace errors remain visible even if an optional reporter fails.
    }
  }

  #assertActive() {
    if (this.destroyed) throw new Error("DesktopWorkspace has been destroyed");
  }

  #sliceSet(slices) {
    if (typeof slices === "string" || !slices || typeof slices[Symbol.iterator] !== "function") {
      throw new TypeError("changedSlices must be an iterable of state slice names");
    }
    const result = new Set();
    for (const slice of slices) {
      if (typeof slice !== "string" || slice.length === 0) {
        throw new TypeError("changedSlices contains an invalid state slice");
      }
      result.add(slice);
    }
    return result;
  }

  #isAffected(moduleSlices, changedSlices) {
    for (const slice of moduleSlices) if (changedSlices.has(slice)) return true;
    return false;
  }
}
