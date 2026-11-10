export let cache: string = "cold";
export function bump(): void {
  cache = "warm";
}
export function load(): void {
  cache = process.env.API_KEY ?? "cold";
}
