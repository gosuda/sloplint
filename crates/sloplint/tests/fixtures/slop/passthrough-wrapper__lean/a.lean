def greet (name : String) (loud : Bool) : String :=
  renderGreeting name loud

def renderGreeting (name : String) (loud : Bool) : String :=
  if loud then name else name
