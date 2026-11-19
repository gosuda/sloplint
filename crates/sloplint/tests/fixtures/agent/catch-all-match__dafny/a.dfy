datatype Tree = Leaf | Node(left: Tree, right: Tree)
method Describe(t: Tree) returns (r: int)
{
  match t
  case Leaf => r := 0;
  case _ => r := 1;
}
