int nested(bool a, bool b, bool c, bool d, bool e) {
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
