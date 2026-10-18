fun loadKnown(clazz: Class<*>): String {
    val field = clazz.getField("name")
    return field.name
}
