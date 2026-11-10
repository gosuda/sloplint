package probe

type Point struct {
	X int
	Y int
	W int
}

type Raw struct {
	X int
	Y int
	W int
}

func Convert(p Raw) Point {
	return Point{X: p.X, Y: p.Y, W: p.W}
}
