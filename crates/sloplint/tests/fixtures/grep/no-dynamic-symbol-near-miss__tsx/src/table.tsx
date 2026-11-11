export function run(table: Record<string, () => void>): void {
  for (const key of ["render", "load"]) {
    table[key]();
  }
}
