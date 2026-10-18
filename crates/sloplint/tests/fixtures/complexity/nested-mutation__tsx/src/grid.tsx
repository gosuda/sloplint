export function gridSum(n: number): number {
  let total = 0;
  for (let i = 0; i < n; i++) {
    for (let j = 0; j < n; j++) {
      for (let k = 0; k < n; k++) {
        total += i + j + k;
      }
    }
  }
  return total;
}
