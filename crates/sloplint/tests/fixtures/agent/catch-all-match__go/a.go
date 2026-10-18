package probe
type Color int
const (
	Red Color = iota
	Green
	Blue
)
func Describe(c Color) string {
	switch c {
	case Red:
		return "red"
	case Green:
		return "green"
	default:
		return "other"
	}
}
