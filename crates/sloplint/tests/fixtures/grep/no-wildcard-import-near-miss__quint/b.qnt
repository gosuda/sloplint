module Lib {
  pure val value = 1
}
module Use {
  import Lib.*
  import Quux(n = 2).*
  pure val x = 1
}
