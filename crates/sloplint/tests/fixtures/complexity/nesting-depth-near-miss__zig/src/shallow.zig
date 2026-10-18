pub fn shallow(a: bool, b: bool, c: bool) i32 {
    if (a) {
        if (b) {
            if (c) {
                return 1;
            }
        }
    }
    return 0;
}
