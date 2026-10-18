package probe
func MoveTo(x int64, y int64) int64 {
	return Place(y, x)
}
func Place(x int64, y int64) int64 {
	return x + y
}
func Fresh() []int64 {
	return make([]int64, 0)
}
