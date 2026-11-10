<?php
enum Color {
    case Red;
    case Blue;
    case Green;
}
function describe($c) {
    switch ($c) {
        case Color::Red:
            return "red";
        case Color::Blue:
            return "blue";
        default:
            return "other";
    }
}
