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
