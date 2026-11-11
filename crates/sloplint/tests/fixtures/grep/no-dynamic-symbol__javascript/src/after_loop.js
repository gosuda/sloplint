const ORDER = ["render", "load"];
export function afterLoop(obj) {
  for (const key of ORDER) {
    obj[key]();
  }
  return obj[key]();
}
