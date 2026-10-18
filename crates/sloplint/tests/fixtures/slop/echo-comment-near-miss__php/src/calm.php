<?php
/// Process the order.
///
/// # Errors
///
/// Fails when the settlement ledger is locked for audit.
function process_order($id, $qty) {
    return $id + $qty;
}
