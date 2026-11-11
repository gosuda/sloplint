export function run(obj: Record<string, (kind: string) => void>, type: string): void {
  const typec = type === "major" ? "red" : "cyan";
  obj[typec](type);
}
