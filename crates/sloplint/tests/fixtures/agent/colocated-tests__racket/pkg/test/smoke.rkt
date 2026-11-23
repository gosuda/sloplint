#lang racket
(require rackunit
         (only-in "../lib/util.rktl" triple)
         "../../../pkg/lib/util.rkt"
         (filtered-in
           (lambda (name) (if (equal? name "../lib/util.rkt") #f name))
           racket/base))

(check-equal? (+ 1 1) 2)
