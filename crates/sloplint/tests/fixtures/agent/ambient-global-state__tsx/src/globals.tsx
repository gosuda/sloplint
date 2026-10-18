export let cache: string = "cold";
export function getCache(): string {
  return cache;
}
const apiKey = process.env.API_KEY;
