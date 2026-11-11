const ORDER = ["render", "load"];
export function run(obj) {
  for (const key of ORDER) {
    obj[key]();
  }
}
