function openConnection(host, retry = true) {
  return host + retry;
}

export function connect(host) {
  return openConnection(host, false);
}
