export function load(key: string): string {
  try {
    return read(key);
  } catch (e) {
    throw new AppError("context", { cause: e });
  }
}
function read(key: string): string {
  return key;
}
class AppError extends Error {
}
