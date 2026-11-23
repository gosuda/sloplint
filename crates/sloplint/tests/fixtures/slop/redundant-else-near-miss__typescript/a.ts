function scopedType(stop: boolean): void {
  type Value = string;
  if (stop) {
    return;
  } else {
    type Value = number;
    const value: Value = 3;
    consume(value);
  }
  const value: Value = "outer";
  consume(value);
}

function decisionChain(value: number): number {
  if (value < 0) {
    return -1;
  } else if (value === 0) {
    return 0;
  } else {
    return 1;
  }
}

function conditionalExit(stop: boolean, ready: boolean): void {
  if (stop) {
    if (ready) return;
  } else {
    proceed();
  }
}

function danglingElse(outer: boolean, stop: boolean): void {
  while (outer)
    if (stop) return;
    else proceed();
}
