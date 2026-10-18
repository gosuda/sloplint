void stepFirst() {
  var alphaValue = computeFirst();
  var betaValue = computeSecond(alphaValue);
  var gammaValue = computeThird(betaValue);
  storeResult(gammaValue);
  finishWork();
}

void stepSecond() {
  var alphaValue = computeFirst();
  var betaValue = computeSecond(alphaValue);
  var gammaValue = computeThird(betaValue);
  storeResult(gammaValue);
  finishWork();
}

int computeFirst() => 1;
int computeSecond(int v) => v;
int computeThird(int v) => v;
void storeResult(int v) {}
void finishWork() {}
