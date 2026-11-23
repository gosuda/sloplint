---- MODULE RedCog ----
Op(a, b) ==
    \E x \in {1, 2} :
        IF a = x THEN
            IF b = x THEN
                IF a = b THEN
                    IF a = 1 THEN TRUE ELSE FALSE
                ELSE FALSE
            ELSE FALSE
        ELSE FALSE
====
