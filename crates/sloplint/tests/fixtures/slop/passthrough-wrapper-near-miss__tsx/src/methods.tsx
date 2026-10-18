export interface Inner {
  len(): number;
}
export class Box {
  private inner: Inner;
  constructor(inner: Inner) {
    this.inner = inner;
  }
  len(): number {
    return this.inner.len();
  }
}
export function flipped(a: string, b: string): string {
  return combine(b, a);
}
function combine(a: string, b: string): string {
  return a + b;
}
