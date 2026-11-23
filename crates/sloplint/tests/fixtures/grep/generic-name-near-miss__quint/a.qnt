module Probe {
  pure def reconcile_ledger = 0
  pure def outer(x) = {
    pure def handle(y) = y + 1
    handle(x)
  }
}
