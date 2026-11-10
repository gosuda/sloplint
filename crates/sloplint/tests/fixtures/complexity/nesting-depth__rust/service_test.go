package probe

import "testing"

func TestNested(t *testing.T) {
    a, b, c, d, e := true, true, true, true, true
    _ = t
    if a {
        if b {
            if c {
                if d {
                    if e {
                        return
                    }
                }
            }
        }
    }
}

func BenchmarkNested(b *testing.B) {
    _ = b
    v, w, x, y, z := true, true, true, true, true
    if v {
        if w {
            if x {
                if y {
                    if z {
                        return
                    }
                }
            }
        }
    }
}

func ExampleNested() {
    a, b, c, d, e := true, true, true, true, true
    if a {
        if b {
            if c {
                if d {
                    if e {
                        return
                    }
                }
            }
        }
    }
}
