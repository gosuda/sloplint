load("//tools:helpers.bzl", "original_helper")
load("//other:lib.bzl", "other_helper")

def f():
    return original_helper(1)
