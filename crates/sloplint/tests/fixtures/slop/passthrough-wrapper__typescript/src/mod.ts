import { inner } from "./inner";
export function wrap(a: string, b: string): string {
  return inner(a, b);
}
export function wrap_with_comment(a: string, b: string): string {
  return inner(a, /* keep */ b);
}
