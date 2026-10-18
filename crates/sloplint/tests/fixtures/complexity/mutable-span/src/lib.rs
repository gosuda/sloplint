pub fn spread(limit: i32) -> i32 {
    let mut total = 0;
    total += 1;
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
    let e = 5;
    let f = 6;
    let g = a + b;
    let h = c + d;
    let i = e + f;
    let j = g + h;
    let k = h + i;
    let m = i + j;
    let n = j + k;
    let o = k + m;
    let p = m + n;
    let q = n + o;
    let r = o + p;
    let s = p + q;
    let t = q + r;
    let u = r + s;
    let v = s + t;
    let w = t + u;
    let x = u + v;
    let y = v + w;
    let z = w + x;
    let sum = x + y + z;
    let fin = sum + a;
    total += fin;
    total += limit;
    total
}
