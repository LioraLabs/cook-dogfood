import type { MenuItem } from "@plateboard/client";

export function renderBoard(items: MenuItem[]): string {
  const rows = items.map(
    (i) => `${i.name.padEnd(24)} $${i.price.toFixed(2)}${i.tags ? `  [${i.tags}]` : ""}`
  );
  return ["== PLATEBOARD ==", ...rows].join("\n");
}
