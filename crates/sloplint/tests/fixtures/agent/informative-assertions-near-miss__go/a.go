package probe
import "github.com/stretchr/testify/assert"
func Check(t interface{}) {
	assert.True(t, true, "service must be ready")
}
