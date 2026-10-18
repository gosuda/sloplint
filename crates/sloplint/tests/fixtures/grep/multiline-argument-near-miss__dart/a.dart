void caller() {
  run(() {
    var a = 1;
    var b = 2;
    print(a + b);
  });
}

void run(void Function() fn) {
  fn();
}
