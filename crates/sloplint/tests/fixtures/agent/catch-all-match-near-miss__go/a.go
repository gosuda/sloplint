package probe
func Classify(code int) string {
	switch code {
	case 0:
		return "zero"
	case 1:
		return "one"
	default:
		return "many"
	}
}
