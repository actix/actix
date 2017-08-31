from __future__ import absolute_import, print_function

import itertools
import os
import random
import signal
import sys
import time

from .. import utils
from .socket import Socket


_sentinel = object()


class Worker(object):

    TYPE = None

    MSG_LOADED = 'loaded'
    MSG_RELOAD = 'reload'
    MSG_RESTART = 'restart'
    MSG_HEARTBEAT = 'hb'
    MSG_CFG_ERROR = 'cfgerror'

    CMD_PREPARE = 'prepare'
    CMD_START = 'start'
    CMD_PAUSE = 'pause'
    CMD_RESUME = 'resume'
    CMD_STOP = 'stop'
    CMD_HEARTBEAT = 'hb'

    ALL_COMMANDS = (CMD_PREPARE, CMD_START,
                    CMD_PAUSE, CMD_RESUME, CMD_STOP, CMD_HEARTBEAT)

    SIGNALS = [getattr(signal, "SIG%s" % x)
               for x in "ABRT HUP QUIT INT TERM USR1 USR2 WINCH CHLD".split()]

    def __init__(self, application, ppid, args):
        self._application = application
        self._ppid = ppid
        self._alive = True
        self._pipe = None
        self._sockets = {}
        self._args = args
        self._on_msg = []
        self._on_shutdown = []

        # service name
        self._name = os.environ.get('FECTL_SRV_NAME')

        # extract master communication pipe
        fd = os.environ.get('FECTL_FD')
        if fd is None:
            raise utils.ConfigurationError(
                "Can not get master process communication FD")

        try:
            self._master_pipe = tuple(int(v) for v in fd.split(':', 1))
        except:
            raise utils.ConfigurationError(
                "Can not decode FECTL_FD_R: %s" % fd)

        self._sockets = Socket.load()

    def get_socket(self, name, default=_sentinel):
        sock = self._sockets.get(name, default)
        if sock is _sentinel:
            raise KeyError(name)

        return sock.socket

    def get_socket_fd(self, name, default=_sentinel):
        try:
            sock = self._sockets[name]
            return sock.socket.fileno()
        except KeyError:
            if default is not _sentinel:
                return default
            raise

    def notify(self, cmd, data=None):
        raise NotImplementedError()

    def heartbeat(self):
        self.notify(self.MSG_HEARTBEAT)

    def on_shutdown(self, cb):
        """ register callback for graceful shutdown process """
        self._on_shutdown.append(cb)

    def _run(self):
        """This is the mainloop of a worker process."""
        raise NotImplementedError()

    def _init_process(self):
        try:
            import setproctitle
            if self._name is not None:
                setproctitle.setproctitle('fectl %s worker' % self._name)
        except:
            pass

        try:
            random.seed(os.urandom(64))
        except NotImplementedError:
            random.seed('%s.%s' % (time.time(), os.getpid()))

        self._pipe = os.pipe()
        for fd in itertools.chain(self._pipe, self._master_pipe):
            utils.close_on_exec(fd)
            utils.set_non_blocking(fd)

        self._f_read = os.fdopen(self._master_pipe[0], 'r')
        self._f_write = os.fdopen(self._master_pipe[1], 'w')

        self._init_signals()

    def _init_signals(self):
        # reset signaling
        [signal.signal(s, signal.SIG_DFL) for s in self.SIGNALS]

        # init new signaling
        signal.signal(signal.SIGQUIT, self._handle_quit)
        signal.signal(signal.SIGTERM, self._handle_exit)
        signal.signal(signal.SIGINT, self._handle_quit)
        signal.signal(signal.SIGWINCH, self._handle_winch)
        signal.signal(signal.SIGUSR1, self._handle_usr1)
        signal.signal(signal.SIGABRT, self._handle_abort)

        # Don't let SIGTERM and SIGUSR1 disturb active requests
        # by interrupting system calls
        signal.siginterrupt(signal.SIGTERM, False)
        signal.siginterrupt(signal.SIGUSR1, False)

        if hasattr(signal, 'set_wakeup_fd'):
            signal.set_wakeup_fd(self._pipe[1])

    def _handle_usr1(self, sig, frame):
        pass

    def _handle_exit(self, sig, frame):
        self._alive = False

    def _handle_quit(self, sig, frame):
        self._alive = False
        time.sleep(0.1)
        sys.exit(0)

    def _handle_abort(self, sig, frame):
        self._alive = False
        sys.exit(1)

    def _handle_winch(self, sig, fname):
        # Ignore SIGWINCH in worker. Fixes a crash on OpenBSD.
        pass
