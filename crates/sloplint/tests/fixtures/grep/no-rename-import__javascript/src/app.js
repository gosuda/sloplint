import { HashMap as Map } from "collections";

export function lookup(key) {
  return new Map().get(key);
}
