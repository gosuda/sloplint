import { readFile } from "node:fs";
import { readFile as readFileAsync } from "node:fs/promises";
import * as path from "node:path";
import { join } from "node:path";
export function load(): string {
  return join("a", "b");
}
