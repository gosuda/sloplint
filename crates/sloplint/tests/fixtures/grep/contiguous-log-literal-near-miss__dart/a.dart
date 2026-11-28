String format(String template) {
  return template;
}

String greeting() {
  return format('a very long literal message here');
}
void check(String message) {
  log /* size < limit */ .flush(message);
}
