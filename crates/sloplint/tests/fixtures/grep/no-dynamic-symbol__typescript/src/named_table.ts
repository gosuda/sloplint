const ORDER: string[] = ["render", "load"];
export function run(obj: Record<string, () => void>): void {
  for (const key of ORDER) {
    obj[key]();
  }
}
