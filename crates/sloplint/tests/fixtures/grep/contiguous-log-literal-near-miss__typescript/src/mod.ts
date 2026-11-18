export function report(path: string): void {
  console.log(`failed to open file ${path}`);
}

export function audit(path: string): void {
  logger.info<string>`failed to open file ${path}`;
}
