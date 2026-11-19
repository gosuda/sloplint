---- MODULE Depth ----
Deep(a) ==
    IF a > 0 THEN
        IF a > 1 THEN
            IF a > 2 THEN
                IF a > 3 THEN
                    IF a > 4 THEN 1 ELSE 0
                ELSE 0
            ELSE 0
        ELSE 0
    ELSE 0
====
