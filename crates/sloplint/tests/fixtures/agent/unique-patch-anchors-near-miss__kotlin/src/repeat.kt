fun stepFirst(input: Int): Int {
    val alpha = computeFirst(input)
    val beta = computeSecond(alpha)
    val gamma = computeThird(beta)
    val delta = computeFourth(gamma)
    val epsilon = computeFifth(delta)
    return storeResult(epsilon) + 1
}

fun stepSecond(input: Int): Int {
    val alpha = computeFirst(input)
    val beta = computeSecond(alpha)
    val gamma = computeThird(beta)
    val delta = computeFourth(gamma)
    val epsilon = computeFifth(delta)
    return storeResult(epsilon) + 2
}
