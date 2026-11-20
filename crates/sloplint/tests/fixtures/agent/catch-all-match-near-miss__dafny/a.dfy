datatype Color = Red | Green
method Name(c: Color) returns (r: int)
{
  match c
  case Red => r := 0;
  case Green => r := 1;
}
