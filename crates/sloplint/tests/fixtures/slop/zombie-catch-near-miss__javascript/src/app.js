export function run(task) {
  try {
    task();
  } catch (e) {
    throw new AppError("task failed", { cause: e });
  }
}
