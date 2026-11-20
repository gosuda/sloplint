// A private paragraph is not part of the searchable API.
module tour/naming

sig Book {}

private pred helper[a: Book] {
  some a
}
