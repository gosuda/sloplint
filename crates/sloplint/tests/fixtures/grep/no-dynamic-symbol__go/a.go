package probe
import "reflect"
func Lookup(v reflect.Value, name string) reflect.Value {
	return v.MethodByName(name)
}
