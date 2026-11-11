pub fn tally(items: &[i32]) -> i32 {
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
    let p = o + n;
    {
        let mut total = 0;
        total += p;
    }
    total + p
}
