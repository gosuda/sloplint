package sample

func ReturnEarly(stop bool) {
	if stop {
		return
	} else {
		proceed()
	}
}

func ContinueEarly(values []int) {
	for _, value := range values {
		if value < 0 {
			continue
		} else {
			consume(value)
		}
	}
}

func BreakEarly(values []int) {
	for _, value := range values {
		if value == 0 {
			break
		} else {
			consume(value)
		}
	}
}
