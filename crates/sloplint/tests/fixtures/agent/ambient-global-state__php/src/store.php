<?php
$GLOBALS["counter"] = 0;
function bump() {
    $GLOBALS["counter"]++;
}
function load() {
    $GLOBALS["counter"] = strlen(getenv("HOME") ?: "");
}
