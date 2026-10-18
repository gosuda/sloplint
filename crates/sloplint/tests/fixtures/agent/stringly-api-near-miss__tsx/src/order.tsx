export interface OrderId {
  value: number;
}
export function processOrder(id: OrderId, qty: number): number {
  return qty;
}
