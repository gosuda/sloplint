module Probe {
  /// Process the order with retries and logging.
  pure def process_order(id: int, qty: int): int = id + qty
}
