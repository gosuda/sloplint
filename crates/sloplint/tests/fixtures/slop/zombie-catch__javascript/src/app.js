export function run(task) {
  try {
    task();
  } catch (e) {
    console.error(e);
    throw e;
  }
}
