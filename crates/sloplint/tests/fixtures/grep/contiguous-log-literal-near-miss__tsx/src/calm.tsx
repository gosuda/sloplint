export function load(path: string): void {
  console.error(`failed to open ${path}: boom`);
}

export function audit(path: string): void {
  logger.warn<string>`failed to open file ${path}`;
}
