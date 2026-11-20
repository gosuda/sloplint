fun g(): Int {
    var x = 0
    x += 1
    var y = 0
    y += 1
    var z = 0
    z += 1
    val a01 = x + 1
    val a02 = x + 2
    val a03 = x + 3
    val a04 = x + 4
    val a05 = x + 5
    val a06 = x + 6
    val a07 = x + 7
    val a08 = x + 8
    val a09 = x + 9
    val a10 = x + 10
    val a11 = x + 11
    val a12 = x + 12
    val a13 = x + 13
    val a14 = x + 14
    // Deliberately parser-level Kotlin (params are `val` and cannot be
    // reassigned): the write to the param name is the only way to
    // exercise whether `fun(x)`'s bare parameter binds `x` at all.
    val f = fun(x) { x = 99 }
    // Comment and annotation delimiters before the param name exercise
    // the same binding path (`fun(/* c */ y)`, `fun(@Anno z)`). The
    // declarations sit at the top so a misattributed write spans more
    // than the strict 15-line minor threshold.
    val g = fun(/* input */ y) { y = 99 }
    val h = fun(@Suppress("a") z) { z = 99 }
    return a14 + a13
}
