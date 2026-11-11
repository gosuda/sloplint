export function process(data: string, count: number): number {
  return data.length + count;
}

export function* values(): Generator<number> {
  yield 1;
}
