export enum Color {
  Red = "red",
  Green = "green",
}
export function describe(color: Color): string {
  switch (color) {
    case Color.Red:
      return "red";
    case Color.Green:
      return "green";
    default:
      return "other";
  }
}
