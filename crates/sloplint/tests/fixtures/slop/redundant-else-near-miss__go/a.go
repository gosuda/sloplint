package sample

func ConditionalExit(stop, ready bool) {
	if stop {
		if ready {
			return
		}
	} else {
		proceed()
	}
}

func ScopedInitializer() {
	if value := read(); value == 0 {
		return
	} else {
		consume(value)
	}
}

func ScopedDeclaration(stop bool) {
	value := "outer"
	if stop {
		return
	} else {
		value := "inner"
		consume(value)
	}
	consume(value)
}

func DecisionChain(value int) {
	if value < 0 {
		return
	} else if value == 0 {
		return
	} else {
		consume(value)
	}
}

func NamedCall(stop bool, panic func(string)) {
	if stop {
		panic("recoverable")
	} else {
		proceed()
	}
}

func EmptyAlternative(stop bool) {
	if stop {
		return
	} else {
		// This branch intentionally has no work.
	}
}
