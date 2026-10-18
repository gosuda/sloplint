package probe
func SumRow(xs []int) int {
	total := 0
	for _, x := range xs {
		total += x
	}
	return total
}
