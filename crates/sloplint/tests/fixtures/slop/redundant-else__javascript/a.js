function returnEarly(stop) {
  if (stop) {
    return;
  } else {
    proceed();
  }
}

function rejectEarly(stop) {
  if (stop) {
    throw new Error("invalid state");
  } else {
    proceed();
  }
}

function visit(values) {
  for (const value of values) {
    if (value < 0) {
      continue;
    } else {
      consume(value);
    }
    if (value === 0) {
      break;
    } else {
      consume(value);
    }
  }
}
