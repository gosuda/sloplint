import { HashMap as Table } from "collections";

export function lookup(key) {
  return new Table().get(key);
}
