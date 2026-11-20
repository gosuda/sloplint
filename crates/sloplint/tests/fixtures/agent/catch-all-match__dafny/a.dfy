datatype Tree = Leaf | Node(left: Tree, right: Tree)
method Describe(t: Tree) returns (r: int)
{
  match t
  case Leaf => r := 0;
  case _ => r := 1;
}
datatype P = P(x: int)
method Read(p: P) returns (r: int)
{
  match p./* note */x
  case 0 => r := 0;
  case _ => r := 1;
}
