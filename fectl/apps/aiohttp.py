"""Aiohttp runner for aiohttp.web"""

import asyncio
import logging
import os
import socket
import ssl

from .. import config, utils, errors
from ..workers import WorkerType
from . import AIOHTTP_SETTINGS


class AiohttpRunner:

    LOG_FORMAT = '%a %l %u %t "%r" %s %b "%{Referrer}i" "%{User-Agent}i"'

    def __init__(self, worker, sock, arguments):  # pragma: no cover
        if worker.TYPE != WorkerType.Asyncio:
            raise errors.UnsupportedWorker(
                'Aiohttp application requires asyncio worker')

        self.loop = asyncio.get_event_loop()
        self.sock = sock
        self.server = None
        self.handler = None
        self.cfg = AIOHTTP_SETTINGS(arguments)

        try:
            self.ssl = self._ssl_context(self.cfg) if self.cfg.is_ssl else None
        except Exception as exc:
            raise utils.ConfigurationError(
                'Can not create ssl context: %s' % exc)

    def make_handler(self, app):
        access_log = self.log.access_log if self.cfg.accesslog else None
        return app.make_handler(
            loop=self.loop,
            # logger=self.log,
            slow_request_timeout=self.cfg.slow_request_timeout,
            keepalive_timeout=self.cfg.keepalive,
            access_log=access_log,
            access_log_format=self.cfg.access_log_format)

    @asyncio.coroutine
    def init(self):
        import aiohttp

        if self.cfg.app is None:
            raise config.ConfigurationError(
                'Aiohttp application is required. '
                'Please provide `app=...` config')

        if isinstance(self.cfg.app, aiohttp.web.Application):
            self.app = self.cfg.app
        else:
            try:
                self.app = self.cfg.app()
                if asyncio.iscoroutine(self.app):
                    self.app = yield from self.app
            except:
                logging.exception('Can not load application')
                raise config.ConfigurationError(
                    'Can not load application: %s' % self.cfg.app)

        yield from self.app.startup()
        self.handler = self.make_handler(self.app)

    @asyncio.coroutine
    def start(self):
        if self.server is None:
            if (hasattr(socket, 'AF_UNIX') and
                    self.sock.family == socket.AF_UNIX):
                self.server = yield from self.loop.create_unix_server(
                    self.handler, sock=self.sock.dup(), ssl=self.ssl)
            else:
                self.server = yield from self.loop.create_server(
                    self.handler, sock=self.sock.dup(), ssl=self.ssl)

    @asyncio.coroutine
    def stop(self):
        if self.server is not None:
            # stop accepting connections
            logging.info("Stopping aiohttp server: %s, connections: %s",
                         os.getpid(), len(self.handler.connections))
            self.server.close()
            yield from self.server.wait_closed()
            self.server = None

        if self.handler is not None:
            # stop alive connections
            yield from self.handler.shutdown(
                timeout=self.cfg.graceful_timeout / 100 * 95)
            self.handler = None

        # send on_shutdown event
        yield from self.app.shutdown()

        # cleanup application
        yield from self.app.cleanup()

    @asyncio.coroutine
    def pause(self):
        if self.server is not None:
            # stop accepting connections
            logging.info("Stop accepting conections: %s", os.getpid())
            self.server.close()
            yield from self.server.wait_closed()
            self.server = None

    @asyncio.coroutine
    def resume(self):
        yield from self.start()

    @staticmethod
    def _ssl_context(cfg):
        """ Creates SSLContext instance for usage in asyncio.create_server.

        See ssl.SSLSocket.__init__ for more details.
        """
        ctx = ssl.SSLContext(cfg.ssl_version)
        ctx.load_cert_chain(cfg.certfile, cfg.keyfile)
        ctx.verify_mode = cfg.cert_reqs
        if cfg.ca_certs:
            ctx.load_verify_locations(cfg.ca_certs)
        if cfg.ciphers:
            ctx.set_ciphers(cfg.ciphers)
        return ctx
