#lang racket
(require rackunit
         (relative-in "../lib/base.rkt"
           (relative-in "helpers/base.rkt"
             (only-in "../util.rkt" double))))

(check-equal? (double 21) 42)
