fun deep(a: Int, flag: Boolean, choice: Int, other: Boolean): Int {
    var x = a
    for (i in 1..10) {
        if (flag) {
            while (x < 100) {
                when (choice) {
                    1 -> {
                        if (other) {
                            x += 1
                        }
                    }
                    else -> {
                        x += 2
                    }
                }
                x += 1
            }
        }
    }
    return x
}
