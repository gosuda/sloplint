<?php
function dispatch($fn, $code) {
    $fn();
    eval($code);
}
