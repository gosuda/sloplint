import assert from "node:assert";
export function check(ok: boolean): void {
  assert(ok, "cache must be warm");
}
