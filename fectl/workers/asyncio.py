import asyncio
import os
import logging
import signal
import struct
import sys

from .. import utils
from . import WorkerType
from .base import Worker


class AsyncioWorker(Worker):

    TYPE = WorkerType.Asyncio

    def __init__(self, app, ppid, args):
        super().__init__(app, ppid, args)

        self._stopping = None
        self._exit_code = 0
        self._loop_type = args.loop
        self._read_queue = None
        self._write_queue = None
        self._notify_waiter = None
        self._apps = []

    def _init_process(self):
        # load event loop
        if self._loop_type == 'default':
            # use default event loop
            pass
        elif self._loop_type == 'uvloop':
            try:
                import uvloop
            except ImportError:
                raise utils.ConfigurationError('uvloop is not available')

            # Setup uvloop policy, so that every
            # asyncio.get_event_loop() will create an instance
            # of uvloop event loop.
            asyncio.set_event_loop_policy(uvloop.EventLoopPolicy())
        elif self._loop_type == 'tokio':
            try:
                import tokio
            except ImportError:
                raise utils.ConfigurationError('tokio is not available')

            # Setup tokio policy, so that every
            # asyncio.get_event_loop() will create an instance
            # of uvloop event loop.
            asyncio.set_event_loop_policy(tokio.EventLoopPolicy())
        else:
            raise utils.ConfigurationError(
                'Unknown loop type: %s' % self._loop_type)

        # create new event_loop after fork
        asyncio.get_event_loop().close()

        loop = asyncio.new_event_loop()
        if self._args.debug:
            loop.set_debug(True)

        self._loop = loop

        # read/write queues to master
        self._read_queue = asyncio.Queue(loop=loop)
        self._write_queue = asyncio.Queue(loop=loop)

        # convert callbacks to coroutine
        self._on_msg = [asyncio.coroutine(cb) for cb in self._on_msg]
        self._on_shutdown = [asyncio.coroutine(cb) for cb in self._on_shutdown]

        for sock in self._sockets.values():
            sock.set_nonblocking()

        asyncio.set_event_loop(loop)
        super()._init_process()

    def notify(self, cmd, data=None):
        self._write_queue.put_nowait(utils.pack_message(cmd, data))

    def _run(self):
        self._read_task = self._loop.create_task(self._read_loop())
        self._write_task = self._loop.create_task(self._write_loop())

        self._runner = asyncio.ensure_future(
            self._try_run_loop(), loop=self._loop)
        try:
            self._loop.run_until_complete(self._runner)
        finally:
            self._loop.close()

        sys.exit(self._exit_code)

    @asyncio.coroutine
    def _try_run_loop(self):
        exc = None
        try:
            yield from self._run_loop()
        except utils.ConfigurationError as e:
            exc = e
            self.notify(self.MSG_CFG_ERROR, str(e))
        except BaseException as e:
            exc = e

        if self._stopping is None:
            self._stopping = asyncio.ensure_future(
                self._stop(), loop=self._loop)

        yield from self._stopping

        if exc is not None:
            raise exc

    @asyncio.coroutine
    def _run_loop(self):
        # init main application
        if self._application is not None:
            try:
                self._application(self)
            except utils.ConfigurationError:
                raise
            except BaseException as exc:
                logging.exception("Application init exception: %s", exc)
                raise

        # load apps
        for sock in self._sockets.values():
            app = sock.load_app(self)
            if app is not None:
                yield from app.init()
                self._apps.append(app)

        self.notify(self.MSG_LOADED)

        try:
            while self._alive:
                self.heartbeat()

                # If our parent changed then we shutdown.
                if self._ppid != os.getppid():
                    self._alive = False
                    logging.info("Parent changed, shutting down: %s", self)
                else:
                    yield from self._wait_next_notify()
        except BaseException:
            logging.exception("Worker run loop exeception")
            pass

    def _wait_next_notify(self):
        self._notify_waiter_done()

        self._notify_waiter = waiter = self._loop.create_future()
        self._loop.call_later(1.0, self._notify_waiter_done)

        return waiter

    def _notify_waiter_done(self):
        waiter = self._notify_waiter
        if waiter is not None and not waiter.done():
            waiter.set_result(True)

        self._notify_waiter = None

    def _init_signals(self):
        # Set up signals through the event loop API.
        self._loop.add_signal_handler(
            signal.SIGQUIT, self._handle_quit, signal.SIGQUIT, None)

        self._loop.add_signal_handler(
            signal.SIGTERM, self._handle_exit, signal.SIGTERM, None)

        self._loop.add_signal_handler(
            signal.SIGINT, self._handle_quit, signal.SIGINT, None)

        self._loop.add_signal_handler(
            signal.SIGWINCH, self._handle_winch, signal.SIGWINCH, None)

        self._loop.add_signal_handler(
            signal.SIGUSR1, self._handle_usr1, signal.SIGUSR1, None)

        self._loop.add_signal_handler(
            signal.SIGABRT, self._handle_abort, signal.SIGABRT, None)

        # Don't let SIGTERM and SIGUSR1 disturb active requests
        # by interrupting system calls
        signal.siginterrupt(signal.SIGTERM, False)
        signal.siginterrupt(signal.SIGUSR1, False)

    @asyncio.coroutine
    def _stop(self):
        # stop accepting connections
        try:
            tasks = [asyncio.ensure_future(app.pause(), loop=self._loop)
                     for app in self._apps]
            yield from asyncio.gather(*tasks, loop=self.loop)
        except:
            pass

        # stop apps
        try:
            tasks = [asyncio.ensure_future(app.stop(), loop=self._loop)
                     for app in self._apps]
            yield from asyncio.gather(*tasks, loop=self.loop)
        except:
            pass

        # on_stop callbacks
        try:
            tasks = [asyncio.ensure_future(cb(), loop=self._loop)
                     for cb in self._on_shutdown]
            yield from asyncio.gather(*tasks, loop=self.loop)
        except:
            pass

        yield from asyncio.sleep(0.1, loop=self._loop)

        self._read_task.cancel()
        self._read_task = None
        self._write_task.cancel()
        self._write_task = None

    def _handle_quit(self, sig, frame):
        if self._stopping is not None:
            self._loop.call_later(0.1, self._notify_waiter_done)
        else:
            self._alive = False

            # init closing process
            self._stopping = asyncio.ensure_future(
                self._stop(), loop=self._loop)

            # close loop
            self._loop.call_later(0.1, self._notify_waiter_done)

    def _handle_abort(self, sig, frame):
        self._alive = False
        self._exit_code = 1
        sys.exit(1)

    def _write_loop(self):
        pipe = os.fdopen(self._master_pipe[1], "w")
        tr, proto = yield from self._loop.connect_write_pipe(
            WriteProtocol, pipe)

        while True:
            try:
                item = yield from self._write_queue.get()
                if not proto.write(item):
                    break
            except (BaseException, RuntimeError, asyncio.CancelledError):
                break
            except Exception:
                logging.exception('Worker write loop exception')
                break

        self._alive = False

    def _read_loop(self):
        pipe = os.fdopen(self._master_pipe[0], "r")
        tr, proto = yield from self._loop.connect_read_pipe(
            ReadProtocol, pipe)
        proto._read_queue = self._read_queue

        while True:
            try:
                cmd, data = yield from self._read_queue.get()
                if cmd == self.CMD_HEARTBEAT:
                    continue

                elif cmd == self.CMD_PAUSE:
                    for app in self._apps:
                        yield from app.pause()

                elif cmd == self.CMD_RESUME:
                    for app in self._apps:
                        yield from app.resume()

                elif cmd == self.CMD_START:
                    for app in self._apps:
                        yield from app.start()

                elif cmd == self.CMD_STOP:
                    # init closing process
                    self._stopping = asyncio.ensure_future(
                        self._stop(), loop=self._loop)

                else:
                    for cb in self._on_msg:
                        try:
                            yield from cb(cmd, data)
                        except:
                            logging.exception('Exception in message handler')
            except asyncio.CancelledError:
                break
            except (Exception, BaseException, RuntimeError):
                logging.exception('Worker write loop exception')
                break

        self._alive = False


class WriteProtocol:

    def __init__(self, *args):
        self._transport = None

    def connection_made(self, transport):
        self._transport = transport

    def connection_lost(self, exc=None):
        self._transport = None

    def write(self, data):
        if self._transport is None:
            return False
        else:
            self._transport.write(data)
            return True


class ReadProtocol:

    def __init__(self, *args):
        self._buf = bytearray()
        self._transport = None
        self._read_queue = None

    def connection_made(self, transport):
        self._transport = transport

    def connection_lost(self, exc=None):
        self._transport = None

    def eof_received(self):
        pass

    def data_received(self, data):
        self._buf += data

        if self._read_queue is not None and len(self._buf) >= 2:
            data = self._buf[:2]
            size = struct.unpack('>h', data)[0]

            if len(self._buf) >= size + 2:
                data = self._buf[2:size+2]
                cmd, data = utils.unpack_message(data)
                self._buf = self._buf[size+2:]
                self._read_queue.put_nowait((cmd, data))
