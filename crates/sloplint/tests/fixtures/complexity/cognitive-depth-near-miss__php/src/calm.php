<?php
function classify($n) {
    if ($n == 0) {
        return "zero";
    } elseif ($n == 1) {
        return "one";
    } elseif ($n == 2) {
        return "two";
    } elseif ($n == 3) {
        return "three";
    } elseif ($n == 4) {
        return "four";
    } elseif ($n == 5) {
        return "five";
    } elseif ($n == 6) {
        return "six";
    } elseif ($n == 7) {
        return "seven";
    } elseif ($n == 8) {
        return "eight";
    } else {
        return "other";
    }
}
