<?php
class Catalog {
    public function convert(object $s): object {
        return new Point(x: $s->x, y: $s->y, z: $s->z, w: $s->w + 1);
    }
}
