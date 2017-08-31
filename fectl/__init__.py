from __future__ import absolute_import, print_function

__version__ = '0.1.2'

import os
import sys


def cmd():
    path = os.path.split(__file__)[0]
    file = os.path.join(path, "fectl")
    if os.path.isfile(file):
        os.execv(file, sys.argv)
    else:
        print("Can not execute fectl")
