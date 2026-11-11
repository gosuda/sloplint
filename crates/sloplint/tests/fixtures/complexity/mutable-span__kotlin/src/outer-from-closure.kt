fun spread(items: List<Int>, limit: Int): Int {
    var total = limit
    total += 1
    val a01 = total + 1
    val a02 = total + 2
    val a03 = total + 3
    val a04 = total + 4
    val a05 = total + 5
    val a06 = total + 6
    val a07 = total + 7
    val a08 = total + 8
    val a09 = total + 9
    val a10 = total + 10
    val a11 = total + 11
    val a12 = total + 12
    val a13 = total + 13
    val a14 = total + 14
    val a15 = total + 15
    val a16 = total + 16
    val a17 = total + 17
    val a18 = total + 18
    val a19 = total + 19
    val a20 = total + 20
    val a21 = total + 21
    val a22 = total + 22
    val a23 = total + 23
    val a24 = total + 24
    val a25 = total + 25
    val a26 = total + 26
    val a27 = total + 27
    val a28 = total + 28
    val tail = a28 + limit
    items.forEach { item ->
        total += item + tail
    }
    return total
}
