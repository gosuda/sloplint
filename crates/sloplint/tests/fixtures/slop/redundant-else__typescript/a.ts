function rejectEarly(stop: boolean): void {
  if (stop) {
    throw new Error("invalid state");
  } else {
    proceed();
  }
}

function returnEarly(stop: boolean): number {
  if (stop) return 0;
  else return read();
}

function loopBoundary(stop: boolean): void {
  if (stop) {
    return;
  } else {
    for (let i = 0; i < 1; i++) {
      work(i)
    }
  }
  (record)();
}

function finalizerBoundary(stop: boolean): void {
  if (stop) {
    return;
  } else {
    try {
      work()
    } finally {
      finish()
    }
  }
  [1].forEach(record);
}
