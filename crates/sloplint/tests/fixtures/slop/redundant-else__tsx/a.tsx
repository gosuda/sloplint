function renderStatus(failed: boolean) {
  if (failed) {
    return <span>Failed</span>;
  } else {
    return <span>Ready</span>;
  }
}
