pub fn spread(items: &[i32], limit: i32) -> i32 {
    let mut total = 0;
    total += 1;
    let a = 1;
    let b = 2;
    let c = a + b;
    let d = c + 1;
    let e = d + 2;
    let f = e + 3;
    let g = f + 4;
    let h = g + 5;
    let i = h + 6;
    let j = i + 7;
    let k = j + 8;
    let m = k + 9;
    let n = m + 10;
    let o = n + 11;
    let p = o + 12;
    let q = p + 13;
    let r = q + 14;
    let s = r + 15;
    let t = s + 16;
    let u = t + 17;
    let v = u + 18;
    let w = v + 19;
    let x = w + 20;
    let y = x + 21;
    let z = y + 22;
    let fin = z + limit;
    let snap = fin + 1;
    let tail = snap + 2;
    items.iter().for_each(|item| {
        total += *item + tail;
    });
    total
}
