---- MODULE DepthNear ----
Shallow(a) ==
    IF a > 0 THEN
        IF a > 1 THEN
            IF a > 2 THEN 1 ELSE 0
        ELSE 0
    ELSE 0
====
