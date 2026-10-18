enum Color { red, green, blue }

String describe(Color c) {
  return switch (c) {
    Color.red => 'red',
    Color.green => 'green',
    Color.blue => 'blue',
  };
}
