pub fn nested(a: bool, b: bool, c: bool, d: bool, e: bool) i32 {
    if (a) {
        if (b) {
            if (c) {
                if (d) {
                    if (e) {
                        return 1;
                    }
                }
            }
        }
    }
    return 0;
}
