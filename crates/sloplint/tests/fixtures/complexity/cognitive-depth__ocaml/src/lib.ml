let tangled (x : int) : int =
  if x > 0 then
    (if x > 1 then
       (if x > 2 then
          (if x > 3 then
             (if x > 4 then
                (if x > 5 then 1 else 2)
              else 3)
           else 4)
        else 5)
     else 6)
  else 0
