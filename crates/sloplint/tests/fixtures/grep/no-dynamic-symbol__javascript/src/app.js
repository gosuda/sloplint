export function dispatch(obj, key) {
  return obj[key]();
}
