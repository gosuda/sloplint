enum Color { red, green, blue }

class Defaults {
  static const String red = 'red';
}

String describe(Color c) {
  return switch (c) {
    Color.red => Defaults.red,
    _ => 'other',
  };
}
