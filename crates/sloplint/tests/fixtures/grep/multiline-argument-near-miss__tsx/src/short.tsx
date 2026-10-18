export function f(): number {
  return compute(() => {
    const a = 1;
    const b = 2;
    return a + b;
  });
}
function compute(fn: () => number): number {
  return fn();
}
