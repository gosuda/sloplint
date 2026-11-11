export function run(obj, type) {
  const typec = type === "major" ? "red" : "cyan";
  return obj[typec](type);
}
