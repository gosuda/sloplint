<?php
function load($path) {
    try {
        return read_file($path);
    } catch (Exception $e) {
        throw new AppError("load context", previous: $e);
    }
}
function read_file($path) {
    return $path;
}
class AppError extends Exception {
}
