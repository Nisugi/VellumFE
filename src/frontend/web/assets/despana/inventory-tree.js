const MAX_TREE_DEPTH = 24;

function validItem(item) {
  return item !== null
    && typeof item === "object"
    && typeof item.id === "string"
    && item.id.length > 0
    && typeof item.relation === "string"
    && typeof item.parent === "string"
    && typeof item.name === "string"
    && typeof item.noun === "string";
}

function uniqueItems(inventoryTree) {
  const items = [];
  const ids = new Set();
  for (const item of Array.isArray(inventoryTree?.items) ? inventoryTree.items : []) {
    if (!validItem(item) || ids.has(item.id)) continue;
    ids.add(item.id);
    items.push(item);
  }
  return items;
}

function linkData(item) {
  return Object.freeze({
    exist_id: item.id,
    noun: item.noun,
    text: item.name,
  });
}

function declaredContainer(item) {
  return Number(item.in_max) > 0 || Number(item.on_max) > 0;
}

/** Project the managed inventory snapshot into a safe personal item tree. */
export function projectInventoryItems({ inventoryTree = null } = {}) {
  if (!inventoryTree || !Array.isArray(inventoryTree.items)) {
    return Object.freeze({
      available: false,
      complete: false,
      truncated: false,
      roots: Object.freeze([]),
    });
  }

  const items = uniqueItems(inventoryTree);
  const children = new Map();
  for (const item of items) {
    if (!children.has(item.parent)) children.set(item.parent, []);
    children.get(item.parent).push(item);
  }

  const rendered = new Set();
  let truncated = false;
  const visit = (item, depth, ancestors) => {
    if (depth > MAX_TREE_DEPTH) {
      truncated = true;
      return null;
    }
    if (ancestors.has(item.id) || rendered.has(item.id)) return null;
    rendered.add(item.id);

    const nextAncestors = new Set(ancestors);
    nextAncestors.add(item.id);
    const projectedChildren = [];
    for (const child of children.get(item.id) || []) {
      const projected = visit(child, depth + 1, nextAncestors);
      if (projected) projectedChildren.push(projected);
    }

    return Object.freeze({
      id: item.id,
      name: item.name,
      noun: item.noun,
      relation: item.relation,
      container: declaredContainer(item) || projectedChildren.length > 0,
      closed: Array.isArray(item.flags) && item.flags.includes("closed"),
      locked: Array.isArray(item.flags) && item.flags.includes("locked"),
      linkData: linkData(item),
      children: Object.freeze(projectedChildren),
    });
  };

  const roots = [];
  for (const item of items) {
    if (item.parent !== "player") continue;
    const projected = visit(item, 0, new Set());
    if (projected) roots.push(projected);
  }

  return Object.freeze({
    available: true,
    complete: inventoryTree.complete === true,
    truncated,
    roots: Object.freeze(roots),
  });
}
