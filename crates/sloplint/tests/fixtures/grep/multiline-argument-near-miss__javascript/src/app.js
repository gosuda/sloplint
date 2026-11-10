import { WalkDir } from "tasks";

export function start() {
  WalkDir("/tmp", (entry) => {
    const a = entry.name;
    const b = entry.size;
    const c = entry.mode;
    const d = entry.path;
    const e = a.length + b;
    const f = c.length + d.length;
    const g = e + f;
    const h = g * 2;
    const i = h - 1;
    const j = i * 3;
    const k = j + 4;
    const l = k * 5;
    const m = l - 6;
    const n = m + 7;
    const o = n * 8;
    return o;
  });
}
