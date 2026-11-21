var counter: Int = 0

def bump(): Unit = {
  counter += 1
}

def load(): Unit = {
  counter = sys.env.get("HOME").map(_.length).getOrElse(0)
}

class Box { var value: Int = 0 }
var box: Box = new Box

def sink(x: Any): Unit = {}

def sink2(other: Any): Unit = {}

var flagged: Int = 0

def store(): Unit = {
  sink(box.value = 4)
  sink(x = 4)
  sink2(flagged = 1)
}
