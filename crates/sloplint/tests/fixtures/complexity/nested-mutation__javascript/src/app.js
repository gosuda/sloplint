export function deepSum(grids) {
  let total = 0;
  for (const grid of grids) {
    for (const row of grid) {
      for (const cell of row) {
        total += cell;
      }
    }
  }
  return total;
}
