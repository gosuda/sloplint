export function report(msg: string): void {
  console.log(msg);
}

export function audit(): void {
  logger.info<string>`short`;
}
