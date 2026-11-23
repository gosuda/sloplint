def return_early(stop):
    if stop:
        return
    else:
        proceed()


def reject_early(stop):
    if stop:
        raise ValueError("invalid state")
    else:
        proceed()


def continue_early(values):
    for value in values:
        if value < 0:
            continue
        else:
            consume(value)


def break_early(values):
    for value in values:
        if value == 0:
            break
        else:
            consume(value)
