import { normalizeHotkey, isCommandMacro, editorToCommand, commandToEditor } from "./macros.js";

// Editing is a thin adapter over Vellum's macro_save/macro_delete messages.
// The server owns TOML persistence; this module never stores executable text
// in localStorage or executes code from the editor.
export class DesktopMacroEditor {
  constructor({ document, macros, dispatch, isOnline }) {
    this.document = document;
    this.macros = macros;
    this.dispatch = dispatch;
    this.isOnline = isOnline;
    this.character = null;
    this.definitions = null;
    this.selected = null;
    this.pending = null;
    this.waitingForDefinitions = true;
    this.dialog = document.createElement("dialog");
    this.dialog.className = "macro-editor";
    this.dialog.setAttribute("aria-labelledby", "macro-editor-heading");
    this.dialog.innerHTML = `
      <header><h2 id="macro-editor-heading">Macros &amp; Hotkeys</h2><button type="button" data-close>Close</button></header>
      <p>Per-character macros, stored in <code>profiles/&lt;character&gt;/macros-local.toml</code>
      under your Vellum data directory. File edits apply with Reload from disk.</p>
      <div class="macro-toolbar"><button type="button" data-new>New macro</button>
      <button type="button" data-reload>Reload from disk</button></div>
      <p data-problems role="status"></p>
      <div class="macro-editor-columns">
        <div class="macro-list" aria-label="Configured macros"></div>
        <form autocomplete="off">
          <label>Name<input name="label" required maxlength="100"></label>
          <label>Group<input name="group" value="Despana" required maxlength="100"></label>
          <label>Hotkey<input name="hotkey" placeholder="ctrl+shift+h (blank = no hotkey)" spellcheck="false"></label>
          <p>Use F2–F4, F7–F9, num_0–num_9, or modifier combinations.
          Browser shortcuts are reserved; some operating systems reserve additional keys.</p>
          <label>Commands<textarea name="command" rows="7" required spellcheck="false" placeholder="stand&#10;s1.5&#10;look"></textarea></label>
          <p>Paste a command block, one command per line. Blank lines are ignored.
          Without explicit delays, commands are sent together; line breaks do not wait for roundtime.
          A line such as <code>s1.5</code> waits 1.5 seconds.
          Semicolons belong to Lich commands such as <code>;bigshot</code>.
          These are fixed delays, not roundtime-aware scripting.</p>
          <label class="macro-confirm"><input name="confirm" type="checkbox">Confirm before running</label>
          <div class="macro-toolbar"><button type="submit">Save</button><button type="button" data-delete disabled>Delete</button></div>
        </form>
      </div>
      <output data-status role="status" aria-live="polite">Hotkeys pause while this editor is open. Saving never runs a macro.</output>`;
    document.body.append(this.dialog);
    this.form = this.dialog.querySelector("form");
    this.list = this.dialog.querySelector(".macro-list");
    this.output = this.dialog.querySelector("[data-status]");
    this.dialog.querySelector("[data-close]").onclick = () => this.dialog.close();
    this.dialog.querySelector("[data-new]").onclick = () => {
      if (!this.formDirty() || this.ask("Discard the editor draft?")) this.edit(null);
    };
    this.dialog.querySelector("[data-reload]").onclick = () => {
      if (this.formDirty() && !this.ask("Reload and discard the editor draft?")) return;
      if (this.send({ kind: "submit-text", text: ".reloadmacros" })) {
        this.edit(null);
        this.output.textContent = "Reload requested; waiting for Vellum. Check Story if it fails.";
      }
    };
    this.form.onsubmit = (event) => { event.preventDefault(); this.save(); };
    this.dialog.querySelector("[data-delete]").onclick = () => this.remove();
    this.dialog.addEventListener("close", () => {
      this.pending = null;
      this.edit(null);
    });
    this.edit(null);
  }

  ask(message) { return this.document.defaultView.confirm(message); }
  field(name) { return this.form.elements.namedItem(name); }
  values() {
    return ["label", "group", "hotkey", "command"].map((name) => this.field(name).value)
      .concat(this.field("confirm").checked);
  }
  formDirty() { return JSON.stringify(this.values()) !== this.baseline; }
  open() {
    if (!this.dialog.open) this.dialog.showModal();
    this.renderList();
  }
  edit(entry) {
    this.selected = entry;
    this.pending = null;
    this.field("label").value = entry?.button.label || "";
    this.field("group").value = entry ? (entry.group || "") : "Despana";
    this.field("hotkey").value = entry?.button.hotkey || "";
    this.field("command").value = commandToEditor(entry?.button.command);
    this.field("confirm").checked = Boolean(entry?.button.confirm);
    this.baseline = JSON.stringify(this.values());
    this.renderAvailability();
  }
  send(intent) {
    if (!this.isOnline()) { this.output.textContent = "Not connected; nothing sent."; return false; }
    try {
      this.dispatch(intent);
      this.output.textContent = "Request sent; waiting for Vellum. Check Story if it fails.";
      return true;
    } catch (error) { this.output.textContent = error.message; return false; }
  }
  save() {
    try {
      const label = this.field("label").value.trim();
      const group = this.field("group").value.trim();
      if (!label || !group) throw new Error("A name and group are required.");
      const hotkey = normalizeHotkey(this.field("hotkey").value);
      const command = editorToCommand(this.field("command").value);
      for (const entry of this.macros.entries) {
        if (this.selected && entry.group === this.selected.group && entry.button.label === this.selected.button.label) continue;
        if (entry.group === group && entry.button.label === label) throw new Error("That group already contains this name.");
        let existingKey;
        try { existingKey = normalizeHotkey(entry.button.hotkey); } catch { continue; }
        if (hotkey && hotkey === existingKey) throw new Error("Hotkey already used by " + entry.button.label + ".");
      }
      const data = { label, group, hotkey: hotkey || null, command, confirm: this.field("confirm").checked,
        color: this.selected?.button.color || null, insert: false, client: null, options: [],
        original: this.selected ? { group: this.selected.group, label: this.selected.button.label } : null };
      if (this.send({ kind: "macro-save", data })) this.pending = { kind: "save", data };
    } catch (error) { this.output.textContent = error.message; }
  }
  remove() {
    if (!this.selected?.button.editable || !this.ask("Delete macro “" + this.selected.button.label + "”?")) return;
    const data = { group: this.selected.group, label: this.selected.button.label };
    if (this.send({ kind: "macro-delete", data })) this.pending = { kind: "delete", data };
  }
  update(view, definitionsReceived = false) {
    const character = view.character || view.session?.character || null;
    if (this.character !== character) {
      this.character = character;
      this.waitingForDefinitions = true;
      this.macros.update(null);
      this.dialog.close();
      this.edit(null);
      this.renderList();
    }
    if (definitionsReceived) this.waitingForDefinitions = false;
    if (view.macros !== this.definitions || definitionsReceived) {
      this.definitions = view.macros;
      this.macros.update(this.waitingForDefinitions ? null : view.macros);
      const pending = this.pending;
      if (pending) {
        const entry = this.macros.entries.find((entry) => entry.group === pending.data.group && entry.button.label === pending.data.label);
        const matched = pending.kind === "delete" ? !entry
          : entry?.button.command === pending.data.command &&
            (entry.button.hotkey || null) === pending.data.hotkey &&
            entry.button.confirm === pending.data.confirm;
        if (matched) {
          this.edit(pending.kind === "save" ? entry : null);
          this.output.textContent = pending.kind === "save" ? "Saved by Vellum. Close the editor to test the hotkey." : "Deleted by Vellum.";
        }
      } else if (this.dialog.open && this.formDirty()) {
        this.output.textContent = "Definitions changed; your unsaved draft is retained. Re-select the macro to load its current version.";
      } else if (this.selected) {
        this.edit(this.macros.entries.find((entry) =>
          entry.group === this.selected.group && entry.button.label === this.selected.button.label) || null);
      }
      this.renderList();
    }
    this.renderAvailability();
  }
  renderAvailability() {
    const ready = this.isOnline() && !this.waitingForDefinitions && this.definitions !== null;
    this.form.querySelector("button[type=submit]").disabled = !ready;
    this.dialog.querySelector("[data-delete]").disabled = !ready || !this.selected?.button.editable;
    this.dialog.querySelector("[data-reload]").disabled = !this.isOnline();
    for (const button of this.list.querySelectorAll("button")) button.disabled = !ready;
  }
  renderList() {
    this.list.replaceChildren();
    this.dialog.querySelector("[data-problems]").textContent = this.macros.problems.join(" ");
    for (const entry of this.macros.entries) {
      const row = this.document.createElement("div");
      const label = this.document.createElement("span");
      label.textContent = (entry.group || "Floating") + " / " + entry.button.label
        + (entry.button.hotkey ? " [" + entry.button.hotkey + "]" : "");
      row.append(label);
      if (isCommandMacro(entry.button)) {
        const run = this.document.createElement("button");
        run.type = "button";
        run.textContent = "Run";
        run.onclick = () => this.macros.activate(entry);
        row.append(run);
        if (entry.button.editable) {
          const edit = this.document.createElement("button");
          edit.type = "button";
          edit.textContent = "Edit";
          edit.onclick = () => {
            if (!this.formDirty() || this.ask("Discard the editor draft?")) this.edit(entry);
          };
          row.append(edit);
        } else {
          const hint = this.document.createElement("small");
          hint.textContent = "Edit in macros.toml, then Reload.";
          row.append(hint);
        }
      } else {
        const hint = this.document.createElement("small");
        hint.textContent = "Menu/type-in/app macro: edit in Vellum's play-page editor.";
        row.append(hint);
      }
      this.list.append(row);
    }
    if (!this.macros.entries.length) this.list.textContent = "No macros yet. Create one to get started.";
    this.renderAvailability();
  }
}
