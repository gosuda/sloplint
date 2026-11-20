module Lib {
  pure val value = 1
}
module Use {
  import Lib.*
  pure val x = 1
}
