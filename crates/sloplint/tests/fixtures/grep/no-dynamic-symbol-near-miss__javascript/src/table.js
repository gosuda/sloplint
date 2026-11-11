export function run(obj) {
  for (const key of ["render", "load"]) {
    obj[key]();
  }
}
