const ORDER: string[] = ["render", "load"];
export function afterLoop(obj: Record<string, () => void>): void {
  for (const key of ORDER) {
    obj[key]();
  }
  obj[key]();
}
