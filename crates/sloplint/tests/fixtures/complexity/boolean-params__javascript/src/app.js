export function openConnection(host, retry = true, secure = false) {
  return host + retry + secure;
}
