<?php
function sum_row($xs) {
    $total = 0;
    foreach ($xs as $x) {
        $total += $x;
    }
    return $total;
}
