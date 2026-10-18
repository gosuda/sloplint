void risky() {
  try {
    work();
  } catch (e) {
    throw FormatException('work failed');
  }
}

void work() {
  print('working through the task');
}
