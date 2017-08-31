from __future__ import absolute_import, print_function

import json
import os
import logging
import socket

from .. import utils


class Socket:

    def __init__(self, name, socket, app, arguments):
        self.name = name
        self.socket = socket
        self.app = app
        self.arguments = arguments

    def set_nonblocking(self):
        self.socket.setblocking(False)

    def load_app(self, worker):
        if self.app is None:
            return None

        app = utils.load_app(self.app)
        try:
            return app(worker, self.socket, self.arguments)
        except utils.ConfigurationError:
            raise
        except:
            logging.exception('Can not initialize app: %s', self.app)
            raise utils.ConfigurationError

        return app

    @classmethod
    def load(cls):
        socks = {}
        apps = {}
        arguments = {}

        for key, value in os.environ.items():
            if key.startswith("FECTL_FD_"):
                params = value.split(',')
                try:
                    fd = int(params[0])
                    params = dict(map(lambda s: s.split(':', 1), params[1:]))
                    family = int(params.get('FAMILY', 0))
                    socktype = int(params.get('SOCKETTYPE', 0))
                    proto = int(params.get('PROTO', 0))
                    sock = socket.fromfd(fd, family, socktype, proto)
                    socks[key[9:]] = sock
                except OSError:
                    raise
                except:
                    raise RuntimeError("Can not decode %s: %s" % (key, value))

            if key.startswith("FECTL_APP_"):
                apps[key[10:]] = value.strip()

            if key.startswith("FECTL_ARGS_"):
                args = {}
                for arg in json.loads(value.strip()):
                    arg = [s.strip() for s in arg.split('=', 1)]
                    if len(arg) == 1:
                        args[arg[0]] = None
                    else:
                        args[arg[0]] = arg[1]

                arguments[key[11:]] = args

        sockets = {}
        for name, sock in socks.items():
            sockets[name] = Socket(
                name, sock, apps.get(name), arguments.get(name, {}))

        return sockets
