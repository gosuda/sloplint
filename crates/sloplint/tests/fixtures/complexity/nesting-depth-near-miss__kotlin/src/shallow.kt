fun shallow(a: Int, flag: Boolean, other: Boolean): Int {
    var x = a
    for (i in 1..10) {
        if (flag) {
            while (x < 100) {
                x += 1
            }
        }
    }
    return x
}
