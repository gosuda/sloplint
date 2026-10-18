import { inner } from "./inner.js";

export function wrap(a, b) {
  return inner(b, a);
}
