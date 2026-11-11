var counter: Int = 0

def bump(): Unit = {
  counter += 1
}

def load(): Unit = {
  counter = sys.env.get("HOME").map(_.length).getOrElse(0)
}
