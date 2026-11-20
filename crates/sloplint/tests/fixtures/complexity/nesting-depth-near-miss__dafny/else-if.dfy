method Pick(x: int) returns (r: int)
{
  if x == 0 { r := 0; }
  else if x == 1 { r := 1; }
  else if x == 2 { r := 2; }
  else if x == 3 { r := 3; }
  else if x == 4 { r := 4; }
  else { r := 5; }
}

function Choose(x: int): int
{
  if x == 0 then 0
  else if x == 1 then 1
  else if x == 2 then 2
  else if x == 3 then 3
  else if x == 4 then 4
  else 5
}
