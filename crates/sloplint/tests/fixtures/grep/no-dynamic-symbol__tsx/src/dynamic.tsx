export function dispatch(table: Record<string, () => void>, name: string): void {
  eval("run");
  table[name]();
}
