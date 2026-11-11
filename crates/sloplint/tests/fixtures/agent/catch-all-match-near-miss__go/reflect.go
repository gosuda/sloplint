package probe
import "reflect"
func KindName(k reflect.Kind) string {
	switch k {
	case reflect.Struct:
		return "struct"
	default:
		return "other"
	}
}
