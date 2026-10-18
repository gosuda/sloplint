<?php
function greet($name, $loud) {
    return render_greeting($loud, $name);
}
function render_greeting($first, $second) {
    return $first;
}
