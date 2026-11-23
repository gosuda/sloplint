int forward(int x, int y) => add(x, y);

int add(int a, int b) => a + b;

class Widget {
  Widget.new(int id);
}

Widget make(int id) => Widget.new(id);
