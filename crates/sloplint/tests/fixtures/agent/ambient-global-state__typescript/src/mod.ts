export let counter = 0;
export function bump(): number {
  counter += 1;
  return counter;
}
