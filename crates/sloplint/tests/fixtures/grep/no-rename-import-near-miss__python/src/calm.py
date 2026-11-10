from math import floor
from decimal import floor as dec_floor
import os
from gettext import gettext as _
from os import path as path


def dummy():
    return (floor, dec_floor, os.name)
