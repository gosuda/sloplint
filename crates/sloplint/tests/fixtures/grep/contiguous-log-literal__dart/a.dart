void logIt(dynamic x) {
  print(x);
}

void loud(String message) {
  Logger.new(message);
}

class Logger {
  Logger.new(String message);
}

void relay(String message) {
  Logger /* outer /* inner */ note */ .new(message);
}
