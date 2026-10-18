<?php
function render_table(bool $header, bool $footer): string {
    $out = "";
    if ($header) {
        $out .= "h";
    }
    if ($footer) {
        $out .= "f";
    }
    return $out;
}
