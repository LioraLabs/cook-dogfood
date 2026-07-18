import { renderBoard } from "@plateboard/ui";
import { listMenu } from "@plateboard/client";
import menu from "./menu.json";

export function board(): string {
  return renderBoard(menu);
}

// Live path: fetch the menu from the running API (services/api) and render it
// with the same board. This uses the generated typed client at runtime, so the
// OpenAPI spec's emitted JavaScript — endpoint paths included — is part of
// this bundle.
export async function liveBoard(baseUrl: string): Promise<string> {
  return renderBoard(await listMenu(baseUrl));
}

console.log(board());
