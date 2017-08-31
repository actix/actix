from __future__ import absolute_import, print_function

import os
import struct
import sys
import logging

import gevent
from gevent.queue import Queue
from gevent.fileobject import FileObjectPosix

from .. import utils
from . import WorkerType
from .base import Worker


class GeventWorker(Worker):

    TYPE = WorkerType.Gevent

    def _patch(self):
        from gevent import monkey
        from gevent.socket import socket as g_socket

        monkey.noisy = False
        monkey.patch_all(subprocess=True)

        # patch sockets
        for name, sock in self._sockets.items():
            s = sock.socket
            if sys.version_info[0] == 3:
                sock.socket = g_socket(
                    s.family, s.type, fileno=s.sock.fileno())
            else:
                sock.socket = g_socket(s.family, s.type, _sock=s)

    def notify(self, cmd, data=None):
        self._write_queue.put(utils.pack_message(cmd, data))

    def _init_process(self):
        # monkey patch here
        self._patch()

        # reinit the hub
        from gevent import hub
        hub.reinit()

        self._write_queue = Queue()
        self._read_queue = Queue()

        # then initialize the process
        super(GeventWorker, self)._init_process()

    def _write_loop(self):
        f = FileObjectPosix(self._master_pipe[1], 'w', 0)

        while True:
            try:
                f.write(self._write_queue.get())
            except:
                self._alive = False
                break

    def _read_loop(self):
        f = FileObjectPosix(self._master_pipe[0], 'r', 0)

        while True:
            try:
                data = f.read(2)
                size = struct.unpack('>h', data)[0]
                data = f.read(size)
                cmd, data = utils.unpack_message(data)
            except:
                # master is dead probably
                self._alive = False
                break

    def _run(self):
        gevent.spawn(self._read_loop)
        gevent.spawn(self._write_loop)

        try:
            self._application(self)
        except BaseException as exc:
            logging.exception("Application init exception: %s", exc)
            raise

        self.notify(self.MSG_LOADED)

        while self._alive:
            self.heartbeat()
            gevent.sleep(1.0)

            if self._ppid != os.getppid():
                logging.info("Parent changed, shutting down")
                self._alive = False
                break

        for cb in self._on_shutdown:
            try:
                cb()
            except BaseException as exc:
                logging.info("Shutdown callback exception: %s", exc)

    def _handle_quit(self, sig, frame):
        # Move this out of the signal handler so we can use blocking calls.
        gevent.spawn(super(GeventWorker, self)._handle_quit, sig, frame)


if __name__ == "__main__":
    worker = GeventWorker()
    worker.init_process()
    worker.run()
