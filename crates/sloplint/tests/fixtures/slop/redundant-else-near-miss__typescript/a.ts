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

function automaticLeadingBoundary(stop: boolean, result: number): number | void {
  if (stop) return result
  else {
    [1].forEach(record);
  }
}

function automaticTrailingCallBoundary(stop: boolean, result: number): void {
  if (stop) {
    return;
  } else {
    record(result)
  }
  (record)(result);
}

function automaticTrailingIndexBoundary(stop: boolean, result: number): void {
  if (stop) {
    return;
  } else {
    record(result)
  }
  [1].forEach(record);
}

function loopHeaderDoesNotEndBody(stop: boolean): void {
  if (stop) {
    return;
  } else {
    for (let i = 0; i < 1; i++)
      work(i)
  }
  (record)();
}

function loopHeaderDoesNotEndAssignment(stop: boolean): void {
  if (stop) {
    return;
  } else {
    for (let i = 0; i < 1; i++)
      result = {}
  }
  [1].forEach(record);
}
