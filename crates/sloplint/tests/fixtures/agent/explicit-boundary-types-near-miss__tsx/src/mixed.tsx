function helper(data) {
  return data;
}
export function process(data: string, count: number): number {
  return count;
}

export function* values(): Generator<number> {
  yield 1;
}

type Handler<E> = (event: E) => number;

export const wrap: <T,>(fn: () => T) => () => T = (fn) => () => fn();

export const handler: Handler<string> = (event) => event.length;
