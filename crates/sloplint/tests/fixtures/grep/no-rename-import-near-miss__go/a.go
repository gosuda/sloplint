package probe
import (
	crand "crypto/rand"
	mrand "math/rand"
	"fmt"
	_ "net/http/pprof"
)
var _ = crand.Reader
var _ = mrand.Int
var _ = fmt.Sprint
func Use() {}
