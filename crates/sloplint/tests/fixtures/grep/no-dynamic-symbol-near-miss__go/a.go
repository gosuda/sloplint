package probe
import "reflect"
func Lookup(v reflect.Value) reflect.Value {
	return v.MethodByName("Foo")
}
