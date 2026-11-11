const ORDER: string[] = ["render", "load"];
export function run(table: Record<string, () => void>): void {
  for (const key of ORDER) {
    table[key]();
  }
}
