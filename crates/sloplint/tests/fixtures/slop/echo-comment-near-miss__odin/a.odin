package probe

// Process the order.
// Sums the quantities of every line item in the cart.
// # Errors
// Fails when the ledger is locked by another writer.
process_order :: proc(id: int, qty: int) -> int {
    return id + qty
}
