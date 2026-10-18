<?php
function load($path) {
    try {
        return read_file($path);
    } catch (Exception $e) {
    }
}
function read_file($path) {
    return $path;
}
