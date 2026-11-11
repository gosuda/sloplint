package probe

import "core:reflect"

lookup_field :: proc(t: reflect.Type_Info) -> rawptr {
    return reflect.struct_field_by_name(t, "name")
}
