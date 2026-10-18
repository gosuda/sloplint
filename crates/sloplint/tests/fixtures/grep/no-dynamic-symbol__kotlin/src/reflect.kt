fun loadField(clazz: Class<*>, fieldName: String): String {
    val field = clazz.getField(fieldName)
    return field.name
}
