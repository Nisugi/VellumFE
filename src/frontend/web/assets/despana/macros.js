// Browser input adapter for Vellum's macro definitions. No command scheduler
// lives here: activation sends an id to Vellum's existing macro dispatcher.
const MODIFIERS = ["ctrl", "alt", "shift", "meta"];
const NAMED_KEYS = new Set(["up", "down", "left", "right", "home", "end", "page_up", "page_down", "insert", "delete"]);
const RESERVED = new Set([
  "ctrl+a", "ctrl+c", "ctrl+x", "ctrl+v", "ctrl+z", "ctrl+y",
  "ctrl+l", "ctrl+t", "ctrl+w", "ctrl+n", "ctrl+r", "ctrl+f", "ctrl+p",
  "ctrl+s", "ctrl+d", "ctrl+h", "ctrl+j", "ctrl+u",
  "ctrl+shift+t", "ctrl+shift+w", "ctrl+shift+n", "ctrl+shift+i",
  "ctrl+shift+j", "ctrl+shift+c", "ctrl+shift+delete",
  "alt+left", "alt+right", "alt+home", "alt+f4",
]);

export function normalizeHotkey(raw) {
  const parts = String(raw || "").trim().toLowerCase().replace(/\s/g, "").split("+");
  if (parts.length === 1 && !parts[0]) return "";
  const key = parts.pop();
  const mods = parts.map((part) => part === "cmd" ? "meta" : part);
  if (new Set(mods).size !== mods.length || mods.some((part) => !MODIFIERS.includes(part))) {
    throw new Error("Use modifiers such as ctrl+shift+h, then one key.");
  }
  if (!/^[a-z0-9]$/.test(key) && !/^f([1-9]|1[0-2])$/.test(key)
      && !/^num_([0-9]|plus|minus|multiply|divide|decimal)$/.test(key) && !NAMED_KEYS.has(key)) {
    throw new Error("Use a letter/number with a modifier, a function key, or a numpad key.");
  }
  const normalized = [...MODIFIERS.filter((part) => mods.includes(part)), key].join("+");
  const browserModifiers = MODIFIERS.filter((part) => part !== "meta" &&
    (mods.includes(part) || (part === "ctrl" && mods.includes("meta"))));
  const browserKey = [...browserModifiers, key].join("+");
  if (!mods.some((part) => part !== "shift") && !/^f\d+$|^num_/.test(key)) {
    throw new Error("Plain typing and navigation keys are reserved for command input.");
  }
  if (["f1", "f5", "f6", "f10", "f11", "f12"].includes(key) ||
      RESERVED.has(browserKey) ||
      /^(ctrl|alt)\+[0-9]$/.test(browserKey) ||
      /^(ctrl|alt)\+num_(plus|minus|decimal|[0-9])$/.test(browserKey)) {
    throw new Error("That shortcut is reserved for the browser or text editing.");
  }
  return normalized;
}

export function eventHotkey(event) {
  if (event.isComposing || event.keyCode === 229 || event.getModifierState?.("AltGraph")) return "";
  const numpad = {
    NumpadAdd: "num_plus", NumpadSubtract: "num_minus", NumpadMultiply: "num_multiply",
    NumpadDivide: "num_divide", NumpadDecimal: "num_decimal",
  };
  const names = { ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right",
    PageUp: "page_up", PageDown: "page_down" };
  const key = /^Numpad[0-9]$/.test(event.code) ? "num_" + event.code.slice(-1)
    : numpad[event.code] || names[event.key] || String(event.key || "").toLowerCase();
  const modifiers = MODIFIERS.filter((mod) => event[mod === "ctrl" ? "ctrlKey" : mod + "Key"]);
  try { return normalizeHotkey([...modifiers, key].join("+")); } catch { return ""; }
}

export function macroEntries(definitions) {
  const entries = [];
  for (const group of definitions?.groups || []) {
    for (const button of group.buttons || []) entries.push({ group: group.name, button });
  }
  for (const button of definitions?.floating || []) entries.push({ group: null, button });
  return entries.filter(({ button }) => typeof button?.id === "string" && typeof button.label === "string");
}

export function isCommandMacro(button) {
  return !button.client && !button.insert && !(button.options?.length);
}

export function commandToEditor(command) {
  return String(command || "").replace(/\r\n?/g, "\n").replace(/\n$/, "");
}

export function editorToCommand(text) {
  const lines = String(text || "").replace(/\r\n?/g, "\n").split("\n").map((line) => line.trim()).filter(Boolean);
  if (!lines.length) throw new Error("Enter at least one command.");
  return lines.join("\r");
}

export class DesktopMacros {
  constructor({ dispatch, isOnline, confirm, status }) {
    this.dispatch = dispatch;
    this.isOnline = isOnline;
    this.confirm = confirm;
    this.status = status;
    this.entries = [];
    this.bindings = new Map();
    this.problems = [];
  }

  update(definitions) {
    this.entries = macroEntries(definitions);
    this.bindings.clear();
    this.problems = [];
    const duplicates = new Set();
    for (const entry of this.entries) {
      if (!entry.button.hotkey) continue;
      let key;
      try { key = normalizeHotkey(entry.button.hotkey); }
      catch (error) { this.problems.push(entry.button.label + ": " + error.message); continue; }
      if (!isCommandMacro(entry.button)) {
        this.problems.push(entry.button.label + ": hotkeys currently support command macros only.");
        continue;
      }
      if (this.bindings.has(key) || duplicates.has(key)) {
        this.bindings.delete(key);
        duplicates.add(key);
        this.problems.push("Duplicate shortcut " + key + " disabled; choose different keys.");
      } else if (key) this.bindings.set(key, entry);
    }
  }

  activate(entry) {
    if (!this.isOnline() || !this.entries.includes(entry) || !isCommandMacro(entry.button)) return false;
    if (entry.button.confirm && !this.confirm("Run macro “" + entry.button.label + "”?")) return false;
    // A confirmation dialog may have yielded while the session changed.
    if (!this.isOnline() || !this.entries.includes(entry)) return false;
    try {
      this.dispatch({ kind: "macro", id: entry.button.id, label: entry.button.label });
      this.status("Sent macro (unconfirmed): " + entry.button.label);
      return true;
    } catch (error) {
      this.status(error.message || "Macro was not sent.");
      return false;
    }
  }

  handleKey(event, { commandInput, blocked = false } = {}) {
    if (blocked || event.defaultPrevented || !this.isOnline()) return false;
    const target = event.target;
    if (target !== commandInput && (target?.isContentEditable || target?.closest?.("input,textarea,select,[role=dialog],dialog"))) return false;
    const entry = this.bindings.get(eventHotkey(event));
    if (!entry) return false;
    event.preventDefault();
    event.stopPropagation();
    if (!event.repeat) this.activate(entry);
    return true;
  }
}
