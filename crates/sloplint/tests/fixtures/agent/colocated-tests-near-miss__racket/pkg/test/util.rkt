#lang racket
(require rackunit
         (only-in "../../pkg/lib/util.rktl" triple)
         (relative-in "../lib/base.rkt"
           (relative-in "helpers/base.rkt"
             (only-in "../util.rkt" double))))

(check-equal? (double 21) 42)
(check-equal? (triple 14) 42)
