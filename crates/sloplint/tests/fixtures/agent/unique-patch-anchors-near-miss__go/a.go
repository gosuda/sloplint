package probe
func StepFirst(input int, output *int) {
	alphaValue := ComputeFirst(input)
	betaValue := ComputeSecond(alphaValue)
	gammaValue := ComputeThird(betaValue)
	deltaValue := ComputeFourth(gammaValue)
	epsilonValue := ComputeFifth(deltaValue)
	StoreResult(output, epsilonValue)
	FinishFirst()
}
func StepSecond(input int, output *int) {
	alphaValue := ComputeFirst(input)
	betaValue := ComputeSecond(alphaValue)
	gammaValue := ComputeThird(betaValue)
	deltaValue := ComputeFourth(gammaValue)
	epsilonValue := ComputeFifth(deltaValue)
	StoreResult(output, epsilonValue)
	FinishSecond()
}
func ComputeFirst(x int) int {
	return x
}
func ComputeSecond(x int) int {
	return x
}
func ComputeThird(x int) int {
	return x
}
func ComputeFourth(x int) int {
	return x
}
func ComputeFifth(x int) int {
	return x
}
func StoreResult(o *int, v int) {}
func FinishFirst() {}
func FinishSecond() {}
