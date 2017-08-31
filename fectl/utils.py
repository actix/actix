from __future__ import absolute_import, print_function

import fcntl
import json
import inspect
import os
import struct
import traceback

from . import apps, workers
from .path import DottedNameResolver
from .errors import ConfigurationError


def set_non_blocking(fd):
    flags = fcntl.fcntl(fd, fcntl.F_GETFL) | os.O_NONBLOCK
    fcntl.fcntl(fd, fcntl.F_SETFL, flags)


def close_on_exec(fd):
    flags = fcntl.fcntl(fd, fcntl.F_GETFD) | fcntl.FD_CLOEXEC
    fcntl.fcntl(fd, fcntl.F_SETFD, flags)


def pack_message(cmd, data=None):
    msg = {'cmd': str(cmd)}
    if data is not None:
        msg['data'] = data

    msg = json.dumps(msg).encode('utf-8')
    return struct.pack('>h', len(msg)) + msg


CMD_PREPARE = 'prepare'
CMD_START = 'start'
CMD_PAUSE = 'pause'
CMD_RESUME = 'resume'
CMD_STOP = 'stop'
CMD_HEARTBEAT = 'hb'

ALL_COMMANDS = (CMD_PREPARE, CMD_START,
                CMD_PAUSE, CMD_RESUME, CMD_STOP, CMD_HEARTBEAT)


def unpack_message(data):
    msg = json.loads(data)
    cmd = msg['cmd']
    if cmd not in ALL_COMMANDS:
        return None, None

    return cmd, None


def load_class(uri):
    if inspect.isclass(uri):
        return uri

    if uri in workers.WORKERS:
        uri, check = workers.WORKERS[uri]
        if check is not None:
            check()

    try:
        return DottedNameResolver().resolve(uri)
    except:
        exc = traceback.format_exc()
        msg = "class uri %r invalid or not found: \n\n[%s]"
        raise ConfigurationError(msg % (uri, exc))


def load_app(uri):
    if uri in apps.APPS:
        uri = apps.APPS[uri]

    try:
        return DottedNameResolver().resolve(uri)
    except:
        exc = traceback.format_exc()
        msg = "loader app %r invalid or not found: \n\n[%s]"
        raise ConfigurationError(msg % (uri, exc))
