<?php
function counter() {
    static $count = 0;
    $count++;
    return $count;
}
function home_dir() {
    return getenv("HOME");
}
