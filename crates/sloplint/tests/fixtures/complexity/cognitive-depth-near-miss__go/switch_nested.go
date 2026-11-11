package probe
func SwitchNested(a int) int {
	x := a
	switch a {
	case 0:
		switch x {
		case 0:
			x = 1
		default:
			x = 2
		}
	default:
		x = -x
	}
	return x
}
