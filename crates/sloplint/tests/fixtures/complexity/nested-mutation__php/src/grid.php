<?php
function grid_sum($n) {
    $total = 0;
    foreach (range(0, $n) as $i) {
        foreach (range(0, $n) as $j) {
            foreach (range(0, $n) as $k) {
                $total += $i + $j + $k;
            }
        }
    }
    return $total;
}
