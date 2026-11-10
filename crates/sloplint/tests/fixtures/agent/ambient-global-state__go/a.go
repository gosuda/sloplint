package probe
import "os"
var Counter int
func Bump() {
	Counter++
}
func Load() {
	Counter = len(os.Getenv("HOME"))
}
