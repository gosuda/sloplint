package probe
import "testing"
func TestRender(t *testing.T) {
	if Render("x") != "x" {
		t.Fail()
	}
}
