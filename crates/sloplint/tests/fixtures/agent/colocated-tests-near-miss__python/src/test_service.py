from service import render


def test_renders():
    assert render("x") == "x"
