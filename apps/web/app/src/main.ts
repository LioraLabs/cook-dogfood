import { renderBoard } from "@plateboard/ui";
import menu from "./menu.json";

export function board(): string {
  return renderBoard(menu);
}

console.log(board());
