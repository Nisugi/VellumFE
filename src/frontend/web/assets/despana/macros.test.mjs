import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
const source = await readFile(new URL("./macros.js", import.meta.url), "utf8");
const { DesktopMacros, normalizeHotkey, eventHotkey, editorToCommand, commandToEditor } =
  await import("data:text/javascript;base64," + Buffer.from(source).toString("base64"));

const definitions = (buttons) => ({ groups: [{ name: "Tests", buttons }], floating: [] });
const button = (overrides = {}) => ({ id: "g:0:b:0", label: "Look", hotkey: "ctrl+shift+h", command: "look", ...overrides });
function setup(buttons = [button()]) {
  const sent = [], statuses = [];
  let online = true, approved = true;
  const macros = new DesktopMacros({ dispatch: (value) => sent.push(value),
    isOnline: () => online, confirm: () => approved, status: (value) => statuses.push(value) });
  macros.update(definitions(buttons));
  return { macros, sent, statuses, offline() { online = false; }, deny() { approved = false; } };
}
function key(overrides = {}) {
  return { key: "H", code: "KeyH", ctrlKey: true, shiftKey: true,
    preventDefault() { this.prevented = true; }, stopPropagation() {}, ...overrides };
}

test("hotkeys normalize without accepting typing, navigation, or browser shortcuts", () => {
  assert.equal(normalizeHotkey(" Shift + CTRL + H "), "ctrl+shift+h");
  assert.equal(normalizeHotkey("cmd+shift+h"), "shift+meta+h");
  assert.equal(normalizeHotkey("num_plus"), "num_plus");
  assert.equal(normalizeHotkey(""), "");
  for (const reserved of ["h", "shift+h", "up", "ctrl+c", "meta+c", "meta+shift+i", "ctrl+1", "alt+9", "ctrl+l", "ctrl+w", "alt+f4", "f5", "f12", "ctrl+ctrl+h"]) {
    assert.throws(() => normalizeHotkey(reserved), reserved);
  }
});
test("key events distinguish numpad and reject IME/AltGraph", () => {
  assert.equal(eventHotkey(key()), "ctrl+shift+h");
  assert.equal(eventHotkey(key({ key: "End", code: "Numpad1", ctrlKey: false, shiftKey: false })), "num_1");
  assert.equal(eventHotkey(key({ isComposing: true })), "");
  assert.equal(eventHotkey(key({ getModifierState: () => true })), "");
});
test("multiline editor preserves Lich commands and Vellum delay segments", () => {
  assert.equal(editorToCommand("stand\n s1.5\n;bigshot\n"), "stand\rs1.5\r;bigshot");
  assert.equal(commandToEditor("stand\rs1.5\rlook\r"), "stand\ns1.5\nlook");
  assert.throws(() => editorToCommand(" \n "));
});
test("large pasted blocks round-trip without truncation or interpreting command text", () => {
  const lines = Array.from({ length: 1000 }, (_, index) =>
    index % 3 === 0 ? `;alias set test${index} say "café ${index}"`
      : index % 3 === 1 ? "s1.5" : `look in my container${index}`);
  for (const newline of ["\n", "\r\n", "\r"]) {
    const pasted = "  " + lines.join(newline + "  ") + newline + newline;
    const command = editorToCommand(pasted);
    assert.equal(command, lines.join("\r"));
    const wire = JSON.parse(JSON.stringify({ command }));
    assert.equal(commandToEditor(wire.command), lines.join("\n"));
    assert.equal(editorToCommand(commandToEditor(wire.command)), command);
  }
});
test("activation uses a server macro id, never reimplements command execution", () => {
  const { macros, sent } = setup();
  const event = key();
  assert.equal(macros.handleKey(event), true);
  assert.equal(event.prevented, true);
  assert.deepEqual(sent, [{ kind: "macro", id: "g:0:b:0", label: "Look" }]);
});
test("held keys, editors, disconnected sessions, and unrecognized keys cannot fire", () => {
  const { macros, sent, offline } = setup();
  macros.handleKey(key({ repeat: true }));
  macros.handleKey(key(), { blocked: true });
  macros.handleKey(key({ target: { closest: () => true } }));
  macros.handleKey(key({ key: "Z", code: "KeyZ" }));
  offline();
  macros.handleKey(key());
  assert.equal(sent.length, 0);
});
test("explicit hotkeys work in command input without consuming ordinary typing", () => {
  const { macros, sent } = setup();
  const input = { closest: () => true };
  assert.equal(macros.handleKey(key({ target: input }), { commandInput: input }), true);
  assert.equal(macros.handleKey(key({ key: "h", ctrlKey: false, shiftKey: false, target: input }), { commandInput: input }), false);
  assert.equal(sent.length, 1);
});
test("duplicate and unsupported file bindings disable instead of selecting a winner", () => {
  const { macros, sent } = setup([button(), button({ id: "g:0:b:1", label: "Hide" }), button({ id: "g:0:b:2" })]);
  macros.handleKey(key());
  assert.equal(sent.length, 0);
  assert.equal(macros.bindings.size, 0);
  assert.ok(macros.problems.some((text) => text.includes("Duplicate")));
  macros.update(definitions([button({ insert: true })]));
  assert.equal(macros.bindings.size, 0);
});
test("cancelled confirmations and stale entries cannot execute", () => {
  const { macros, sent, deny } = setup([button({ confirm: true })]);
  deny();
  macros.handleKey(key());
  const old = macros.entries[0];
  macros.update(definitions([button({ id: "g:0:b:1", label: "New" })]));
  assert.equal(macros.activate(old), false);
  assert.equal(sent.length, 0);
});
test("live definitions replace bindings; clearing definitions disables all hotkeys", () => {
  const { macros, sent } = setup();
  macros.update(definitions([button({ hotkey: "f7" })]));
  macros.handleKey(key());
  assert.equal(sent.length, 0);
  macros.handleKey(key({ key: "F7", code: "F7", ctrlKey: false, shiftKey: false }));
  assert.equal(sent.length, 1);
  macros.update(null);
  assert.equal(macros.bindings.size, 0);
});
