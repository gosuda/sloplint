export interface Point {
  x: number;
  y: number;
  z: number;
  w: number;
}
export function copy(src: Point): Point {
  return { x: src.x, y: src.y, z: src.z, w: src.w };
}
export function copy_after_comment(src: Point): Point {
  return { /* lead */ x: src.x, y: src.y, z: src.z, w: src.w };
}
