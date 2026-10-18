export enum Color {
  Red = "red",
  Blue = "blue",
}
export function name(c: Color): string {
  switch (c) {
    case Color.Red:
      return "red";
    default:
      return "other";
  }
}
