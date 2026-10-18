export function f01(): number {
  return 1;
}
export function f02(): number {
  return 2;
}
export function f03(): number {
  return 3;
}
export function f04(): number {
  return 4;
}
export function f05(): number {
  return 5;
}
export function f06(): number {
  return 6;
}
export function f07(): number {
  return 7;
}
export function f08(): number {
  return 8;
}
export interface CardProps {
  title: string;
}
export function PreviewCard(props: CardProps): unknown {
  return <div className="card">{props.title}</div>;
}
