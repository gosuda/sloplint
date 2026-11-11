package probe
func Compute(vals []int) int {
	total := 0
	for _, v := range vals {
		total += v
	}
	return total
}
func F() int {
	return Compute(
		[]int{
			1,
			2,
			3,
			4,
			5,
			6,
			7,
			8,
			9,
			10,
			11,
			12,
			13,
			14,
			15,
		},
	)
}
