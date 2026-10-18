<?php
enum Color {
    case Red;
    case Blue;
}
function describe($c) {
    switch ($c) {
        case Color::Red:
            return "red";
        default:
            return "other";
    }
}
