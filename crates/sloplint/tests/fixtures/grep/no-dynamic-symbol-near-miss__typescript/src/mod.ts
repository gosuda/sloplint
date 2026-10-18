export function run(obj: Record<string, () => void>): void {
  obj["key"]();
}
