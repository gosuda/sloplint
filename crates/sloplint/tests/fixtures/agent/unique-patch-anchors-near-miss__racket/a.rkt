#lang racket
(provide step-first step-second)
(define (step-first input output)
  (define alpha-value (compute-first input))
  (store-result output alpha-value))
(define (step-second input output)
  (define beta-value (compute-second input))
  (store-result output beta-value))
