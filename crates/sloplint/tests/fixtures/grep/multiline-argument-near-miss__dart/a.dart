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

void ordered() {
  final result = build([
    1,
    2,
    3,
    4,
    5,
    6,
    7,
    8,
    9,
    10,
    11,
    12,
    13,
    14,
    15,
    16,
  ])..sort();
  print(result);
}

List<int> build(List<int> xs) {
  return xs;
}
