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
