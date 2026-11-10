class Status:
    ACTIVE = "active"
    IDLE = "idle"


def describe(status):
    match status:
        case Status.ACTIVE:
            return "active"
        case Status.IDLE:
            return "idle"
        case _:
            return "other"
