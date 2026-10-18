export function label(code: number): string {
  switch (code) {
    case 1:
      return "one";
    case 2:
      return "two";
    default:
      return "other";
  }
}
