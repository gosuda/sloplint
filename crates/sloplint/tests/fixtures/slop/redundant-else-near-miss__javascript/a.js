function conditionalExit(stop, ready) {
  if (stop) {
    if (ready) return;
  } else {
    proceed();
  }
}

function scopedDeclaration(stop) {
  let value = "outer";
  if (stop) {
    return;
  } else {
    let value = "inner";
    consume(value);
  }
  consume(value);
}

function scopedResource(stop) {
  if (stop) {
    return;
  } else {
    using resource = acquire();
    consume(resource);
  }
  afterDisposal();
}

function decisionChain(value) {
  if (value < 0) {
    return;
  } else if (value === 0) {
    return;
  } else {
    consume(value);
  }
}

function danglingElse(outer, stop) {
  if (outer)
    if (stop) return;
    else proceed();
}

function namedCall(stop) {
  if (stop) {
    fatal("recoverable");
  } else {
    proceed();
  }
}

function emptyAlternative(stop) {
  if (stop) {
    return;
  } else {
    // This branch intentionally has no work.
  }
}

function automaticLeadingBoundary(stop, result) {
  if (stop) return result
  else {
    [1].forEach(record);
  }
}

function automaticTrailingCallBoundary(stop, result) {
  if (stop) {
    return;
  } else {
    record(result)
  }
  (record)(result);
}

function automaticTrailingIndexBoundary(stop, result) {
  if (stop) {
    return;
  } else {
    record(result)
  }
  [1].forEach(record);
}

function loopHeaderDoesNotEndBody(stop) {
  if (stop) {
    return;
  } else {
    for (let i = 0; i < 1; i++)
      work(i)
  }
  (record)();
}

function loopHeaderDoesNotEndAssignment(stop) {
  if (stop) {
    return;
  } else {
    for (let i = 0; i < 1; i++)
      result = {}
  }
  [1].forEach(record);
}
