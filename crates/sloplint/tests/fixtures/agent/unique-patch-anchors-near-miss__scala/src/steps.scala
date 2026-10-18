object Steps {
  def stepFirst(input: Int): Int = {
    val alphaValue = computeFirst(input)
    val betaValue = computeSecond(alphaValue)
    val gammaValue = computeThird(betaValue)
    val deltaValue = computeFourth(gammaValue)
    val epsilonValue = computeFifth(deltaValue)
    storeResult(epsilonValue)
    finishFirst()
  }

  def stepSecond(input: Int): Int = {
    val alphaValue = computeFirst(input)
    val betaValue = computeSecond(alphaValue)
    val gammaValue = computeThird(betaValue)
    val deltaValue = computeFourth(gammaValue)
    val epsilonValue = computeFifth(deltaValue)
    storeResult(epsilonValue)
    finishSecond()
  }
}
