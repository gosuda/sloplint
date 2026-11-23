def conditional_exit(stop, ready):
    if stop:
        if ready:
            return
    else:
        proceed()


def decision_chain(value):
    if value < 0:
        return
    elif value == 0:
        return
    else:
        consume(value)


def named_call(stop):
    if stop:
        fatal("recoverable")
    else:
        proceed()


def nested_function(stop):
    if stop:
        def deferred():
            return
    else:
        proceed()


def loop_may_be_empty(stop, values):
    if stop:
        for value in values:
            return value
    else:
        proceed()


def try_else_is_distinct():
    try:
        work()
    except ValueError:
        raise
    else:
        proceed()
