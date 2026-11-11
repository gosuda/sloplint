package probe
import "errors"
func Fail() error {
	return errors.New("fail")
}
