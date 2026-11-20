var counter: Int = 0

def bump(): Unit = {
  counter += 1
}

def load(): Unit = {
  counter = sys.env.get("HOME").map(_.length).getOrElse(0)
}

class Box { var value: Int = 0 }
var box: Box = new Box

def sink(x: Int): Unit = {}

def store(): Unit = {
  sink(box.value = 4)
  sink(counter = 4)
}
