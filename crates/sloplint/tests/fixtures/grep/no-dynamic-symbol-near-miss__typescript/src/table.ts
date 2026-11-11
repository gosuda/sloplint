export function run(obj: Record<string, () => void>): void {
  for (const key of ["render", "load"]) {
    obj[key]();
  }
}
