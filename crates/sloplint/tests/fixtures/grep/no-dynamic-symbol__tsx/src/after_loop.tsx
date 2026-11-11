const ORDER: string[] = ["render", "load"];
export function afterLoop(table: Record<string, () => void>): void {
  for (const key of ORDER) {
    table[key]();
  }
  table[key]();
}
