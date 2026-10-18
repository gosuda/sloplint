fun stepFirst(input: Int): Int {
    val alpha = computeFirst(input)
    val beta = computeSecond(alpha)
    val gamma = computeThird(beta)
    val delta = computeFourth(gamma)
    val epsilon = computeFifth(delta)
    val result = storeResult(epsilon)
    return result + 1
}

fun stepSecond(input: Int): Int {
    val alpha = computeFirst(input)
    val beta = computeSecond(alpha)
    val gamma = computeThird(beta)
    val delta = computeFourth(gamma)
    val epsilon = computeFifth(delta)
    val result = storeResult(epsilon)
    return result + 2
}

fun stepThird(input: Int): Int {
    val alpha = computeFirst(input)
    val beta = computeSecond(alpha)
    val gamma = computeThird(beta)
    val delta = computeFourth(gamma)
    val epsilon = computeFifth(delta)
    val result = storeResult(epsilon)
    return result + 3
}

