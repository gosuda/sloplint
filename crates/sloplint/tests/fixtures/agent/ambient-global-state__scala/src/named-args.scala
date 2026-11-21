var `type` = "legacy"

case class Config(`type`: String)

def config(): Config = {
  // A named argument is an assignment_expression in the grammar but only
  // sets the constructor field — it must not count as a write to `type`.
  Config(`type` = "modern")
}
