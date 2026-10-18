export function render(label: string, visible: boolean, active: boolean): string {
  return visible && active ? label : "";
}
