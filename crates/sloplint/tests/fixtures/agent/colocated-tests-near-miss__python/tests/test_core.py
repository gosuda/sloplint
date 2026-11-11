from pkg.core import render


def test_renders():
    assert render("x") == "x"
