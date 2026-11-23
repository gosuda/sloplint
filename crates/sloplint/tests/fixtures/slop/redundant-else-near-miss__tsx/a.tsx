function renderStatus(state: string) {
  if (state === "error") {
    return <span>Failed</span>;
  } else if (state === "loading") {
    return <span>Loading</span>;
  } else {
    return <span>Ready</span>;
  }
}

function renderValue(hidden: boolean) {
  const value = "outer";
  if (hidden) {
    return null;
  } else {
    const value = "inner";
    consume(<span>{value}</span>);
  }
  return <span>{value}</span>;
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
