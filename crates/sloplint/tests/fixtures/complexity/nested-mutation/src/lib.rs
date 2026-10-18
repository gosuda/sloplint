pub fn grid_sum(n: i32) -> i32 {
    let mut total = 0;
    for i in 0..n {
        for j in 0..n {
            for k in 0..n {
                total += i + j + k;
            }
        }
    }
    total
}
