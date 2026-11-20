method Nested(a: bool, b: bool, c: bool, d: bool, e: bool) returns (r: int)
{
  if a {
    if b {
      if c {
        if d {
          if e {
            r := 1;
          }
        }
      }
    }
  }
  r := 0;
}
