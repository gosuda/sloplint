package probe

// Process the order.
process_order :: proc(id: int, qty: int) -> int {
    return id + qty
}
