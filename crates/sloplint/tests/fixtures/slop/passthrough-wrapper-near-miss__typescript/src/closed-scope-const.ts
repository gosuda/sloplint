function wrap(s: string): string {
  return format(s);
}
function main(): void {
  { const wrap = () => 1; } use(wrap);
}
