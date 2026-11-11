export function run(table: Record<string, (kind: string) => void>, type: string): void {
  const typec = type === "major" ? "red" : "cyan";
  table[typec](type);
}
