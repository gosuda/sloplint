#lang racket
(require rackunit
         "../lib/util.rkt"
         (only-in "../lib/util.rkt" double))

(check-equal? (double 21) 42)
