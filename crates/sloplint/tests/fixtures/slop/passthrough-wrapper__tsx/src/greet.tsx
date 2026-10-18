export function greet(name: string, loud: boolean): string {
  return renderGreeting(name, loud);
}
function renderGreeting(name: string, loud: boolean): string {
  if (loud) {
    return name.toUpperCase();
  }
  return name;
}
