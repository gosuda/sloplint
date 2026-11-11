export function load(key: string): string {
  try {
    return read(key);
  } catch (e) {
    console.error(e);
    throw e;
  }
}
function read(key: string): string {
  return key;
}
