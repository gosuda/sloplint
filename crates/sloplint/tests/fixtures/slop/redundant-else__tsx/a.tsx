function renderStatus(failed: boolean) {
  if (failed) {
    return <span>Failed</span>;
  } else {
    return <span>Ready</span>;
  }
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
