import { strict as assert } from "node:assert";
import { board, liveBoard } from "../dist/bundle.js";

const text = board();
assert.ok(text.includes("PLATEBOARD"), "board header missing");
assert.ok(text.split("\n").length > 1, "no menu rows rendered");
assert.equal(typeof liveBoard, "function", "generated client path missing from bundle");
console.log("smoke ok");
