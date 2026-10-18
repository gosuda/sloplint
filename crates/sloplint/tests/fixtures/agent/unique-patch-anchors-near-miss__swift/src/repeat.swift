public func stepFirst(input: Int, output: inout Int) {
    let alphaValue = computeFirst(input)
    let betaValue = computeSecond(alphaValue)
    let gammaValue = computeThird(betaValue)
    let deltaValue = computeFourth(gammaValue)
    let epsilonValue = computeFifth(deltaValue)
    storeResult(output, epsilonValue)
    finishFirst()
}

public func stepSecond(input: Int, output: inout Int) {
    let alphaValue = computeFirst(input)
    let betaValue = computeSecond(alphaValue)
    let gammaValue = computeThird(betaValue)
    let deltaValue = computeFourth(gammaValue)
    let epsilonValue = computeFifth(deltaValue)
    storeResult(output, epsilonValue)
    finishSecond()
}

