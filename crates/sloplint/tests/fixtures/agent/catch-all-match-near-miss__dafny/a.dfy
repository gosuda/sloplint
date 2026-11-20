datatype Color = Red | Green
method Name(c: Color) returns (r: int)
{
  match c
  case Red => r := 0;
  case Green => r := 1;
}
// Every variant listed: no wildcard arm to flag.
datatype P = P(x: int)
method Update(p: P) returns (r: int)
{
  match p.(x := 1)
  case P(0) => r := 0;
  case _ => r := 1;
}
// Datatype update computes a new value: the scrutinee is Other, not a
// field access, so a wildcard arm stays silent.
method Negate(p: P) returns (r: int)
{
  match -(p./* note */x)
  case 0 => r := 0;
  case _ => r := 1;
}
// Negation wraps the field access in a computing operator: Other again.
