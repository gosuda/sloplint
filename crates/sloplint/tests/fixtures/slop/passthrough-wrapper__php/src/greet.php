<?php
function greet($name, $loud) {
    return render_greeting($name, $loud);
}
function render_greeting($name, $loud) {
    return $name;
}
