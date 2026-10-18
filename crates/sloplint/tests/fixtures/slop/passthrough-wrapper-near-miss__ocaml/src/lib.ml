let inner (x : int) (y : int) : int = x + y
let wrapper (x : int) (y : int) : int = inner y x
