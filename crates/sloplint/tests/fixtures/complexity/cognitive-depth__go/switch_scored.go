package probe
func SwitchScored(a int) int {
	x := a
	switch a {
	case 0:
		for i := 0; i < a; i++ {
			if i > 0 {
				x += 1
			}
			if i > 1 {
				x += 2
			}
			if i > 2 {
				x += 3
			}
			if i > 3 {
				x += 4
			}
			if i > 4 {
				x += 5
			}
			if i > 5 {
				x += 6
			}
			if i > 6 {
				x += 7
			}
			if i > 7 {
				x += 8
			}
			if i > 8 {
				x += 9
			}
			if i > 9 {
				x += 10
			}
		}
	default:
		x = -x
	}
	return x
}
