final String apiUrl = 'https://example.com';

String endpoint(String path) {
  return '$apiUrl/$path';
}

var handler = () => false;

void setup() {
  final run = (bool handler()) {
    handler = () => true;
  };
  run(() => false);
}
