package probe
func GridSum(n int) int {
	total := 0
	for i := 0; i < n; i++ {
		for j := 0; j < n; j++ {
			for k := 0; k < n; k++ {
				total += i + j + k
			}
		}
	}
	return total
}
