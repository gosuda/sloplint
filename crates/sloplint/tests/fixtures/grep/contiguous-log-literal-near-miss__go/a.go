package probe
import "log"
func Load(path string, reason string) {
	log.Printf("failed to open %s: %s", path, reason)
}
