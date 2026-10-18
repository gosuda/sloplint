export function dispatch(table: Record<string, () => void>): void {
  table["run"]();
}
