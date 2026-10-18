package probe
import (
	"log"
	"os"
)
func Load(path string) (*os.File, error) {
	f, err := os.Open(path)
	if err != nil {
		log.Printf("open failed")
		return nil, err
	}
	return f, nil
}
