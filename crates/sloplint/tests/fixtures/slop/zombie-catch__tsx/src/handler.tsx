export function load(path: string): string {
  try {
    return read(path);
  } catch (e) {
    console.error(e);
    throw e;
  }
}
function read(path: string): string {
  return path;
}
