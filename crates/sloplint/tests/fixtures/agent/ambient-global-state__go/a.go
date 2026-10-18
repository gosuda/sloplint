package probe
import "os"
var Counter int
func Use() string {
	return os.Getenv("HOME")
}
