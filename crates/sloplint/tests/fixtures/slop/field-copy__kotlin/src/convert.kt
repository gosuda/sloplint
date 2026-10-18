data class User(val name: String, val age: Int, val email: String, val phone: String)
data class UserDto(val name: String, val age: Int, val email: String, val phone: String)

fun convert(user: User): UserDto {
    return UserDto(name = user.name, age = user.age, email = user.email, phone = user.phone)
}
