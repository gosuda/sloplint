export function parity(n: number): string {
  switch (n) {
    case 0:
      return "even";
    default:
      return "odd";
  }
}
