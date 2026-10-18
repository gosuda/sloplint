import { inner } from "./inner";
export function wrap(a: string, b: string): string {
  return inner(b, a);
}
