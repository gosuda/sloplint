export interface Point {
  x: number;
  y: number;
  z: number;
  w: number;
}
export function convert(p: Point): Point {
  return {
    x: p.x,
    y: p.y,
    z: p.z,
    w: p.w,
  };
}
