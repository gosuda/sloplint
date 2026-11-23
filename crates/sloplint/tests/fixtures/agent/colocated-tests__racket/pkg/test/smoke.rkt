#lang racket
(require rackunit
         (filtered-in
           (lambda (name) (if (equal? name "../lib/util.rkt") #f name))
           racket/base))

(check-equal? (+ 1 1) 2)
