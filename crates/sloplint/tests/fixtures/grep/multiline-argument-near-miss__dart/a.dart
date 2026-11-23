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

void serve(String? value) {
  consume({
    'alpha': ?value,
    'beta': ?value,
    'gamma': ?value,
    'delta': ?value,
    'epsilon': ?value,
    'zeta': ?value,
    'eta': ?value,
    'theta': ?value,
    'iota': ?value,
    'kappa': ?value,
    'lambda': ?value,
    'mu': ?value,
    'nu': ?value,
    'xi': ?value,
    'omicron': ?value,
  });
}

void consume(Map<String, String?> xs) {
  print(xs);
}
