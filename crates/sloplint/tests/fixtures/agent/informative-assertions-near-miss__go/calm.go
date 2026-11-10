package probe
import "github.com/stretchr/testify/assert"
func Check(t interface{}, h *Helper) {
	assert.True(t, h.called)
}
type Helper struct{ called bool }
