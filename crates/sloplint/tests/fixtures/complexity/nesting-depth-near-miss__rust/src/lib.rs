pub fn check(a: bool, b: bool, c: bool, d: bool, opt: Option<i32>) -> bool {
    if a {
        if b {
            if c {
                if d {
                    return opt.is_some_and(|v| v > 0);
                }
            }
        }
    }
    false
}
