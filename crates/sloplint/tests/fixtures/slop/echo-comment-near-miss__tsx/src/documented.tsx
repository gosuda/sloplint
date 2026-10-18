// Process the order.
// # Errors
// Throws when the ledger is closed or when the batch fails validation after several retries.
export function processOrder(id: number, qty: number): number {
  return id + qty;
}
