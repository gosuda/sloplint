package probe
func Compute(vals []int) int {
	return len(vals)
}
func F() int {
	return Compute(
		[]int{
			1,
			2,
		},
	)
}
