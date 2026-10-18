export function accumulate(grid: number[][]): number {
  let total = 0;
  for (const row of grid) {
    for (const v of row) {
      for (let i = 0; i < v; i++) {
        total += v;
      }
    }
  }
  return total;
}
