import assert from "node:assert/strict";
import test from "node:test";

import { projectInventoryItems } from "./inventory-tree.js";

function item(id, relation, parent, name = id, noun = id, extra = {}) {
  return {
    id,
    relation,
    parent,
    name,
    noun,
    in_max: null,
    on_max: null,
    flags: [],
    ...extra,
  };
}

test("Inventory projects personal items as a guarded nested tree", () => {
  const result = projectInventoryItems({
    inventoryTree: {
      room: "100",
      complete: true,
      generation: 4,
      items: [
        item("staff", "righthand", "player", "a runestaff", "runestaff"),
        item("pack", "worn", "player", "a weathered pack", "pack", { in_max: 2000 }),
        item("pouch", "in", "pack", "a velvet pouch", "pouch", { in_max: 500 }),
        item("gem", "in", "pouch", "a smoky gem", "gem"),
        item("orphan", "in", "missing", "a lost key", "key"),
        item("cycle-a", "in", "cycle-b"),
        item("cycle-b", "in", "cycle-a"),
        item("room-table", "room", "room", "a stone table", "table"),
      ],
    },
  });

  assert.equal(result.available, true);
  assert.equal(result.complete, true);
  assert.equal(result.truncated, false);
  assert.deepEqual(result.roots.map((entry) => entry.id), ["staff", "pack"]);
  assert.equal(result.roots[0].container, false);
  assert.equal(result.roots[1].container, true);
  assert.equal(result.roots[1].children[0].id, "pouch");
  assert.equal(result.roots[1].children[0].container, true);
  assert.equal(result.roots[1].children[0].children[0].id, "gem");
  assert.equal(Object.isFrozen(result.roots[1].children), true);
});

test("Inventory rejects duplicates and bounds malicious nesting", () => {
  const items = [item("depth-0", "worn", "player", "a pack", "pack", { in_max: 2000 })];
  for (let depth = 1; depth < 60; depth += 1) {
    items.push(item(`depth-${depth}`, "in", `depth-${depth - 1}`));
  }
  items.push(item("depth-1", "in", "depth-0", "duplicate"));

  const result = projectInventoryItems({
    inventoryTree: { room: "100", complete: false, generation: 3, items },
  });
  assert.equal(result.available, true);
  assert.equal(result.complete, false);
  assert.equal(result.truncated, true);
  assert.equal(result.roots.length, 1);
});
