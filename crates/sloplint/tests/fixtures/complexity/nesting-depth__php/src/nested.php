<?php
function nested($a, $b, $c, $d, $e) {
    if ($a) {
        if ($b) {
            if ($c) {
                if ($d) {
                    if ($e) {
                        return 1;
                    }
                }
            }
        }
    }
    return 0;
}
