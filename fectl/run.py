from __future__ import absolute_import, print_function

import argparse
import logging
import os
import sys

from . import utils
from .path import DottedNameResolver
from .utils import ConfigurationError

WORKER_INIT_FAILED = 99
WORKER_BOOT_FAILED = 100

ARGS = argparse.ArgumentParser(description='FECTL Worker')
SUBPARSERS = ARGS.add_subparsers(
    title='Worker type',
    description='Worker type to use (i.e. asyncio, gevent, etc)',
    dest='worker')

ARGS_GEVENT = SUBPARSERS.add_parser('gevent', help='Gevent based worker')
ARGS_GEVENT.add_argument('--app', dest='app', action='store', required=False,
                         help='Application to start')

ARGS_ASYNCIO = SUBPARSERS.add_parser('asyncio', help='Asyncio based worker')
ARGS_ASYNCIO.add_argument('--loop', dest='loop', action='store',
                          default="default", required=False,
                          choices=['default', 'uvloop', 'tokio'],
                          help='Select asyncio event loop')
ARGS_ASYNCIO.add_argument('--app', dest='app', action='store', required=False,
                          help='Application to start')
ARGS_ASYNCIO.add_argument('--debug', dest='debug', action='store_true',
                          required=False, default=False,
                          help='Enable event loop debug mode')


def run():
    try:
        args = ARGS.parse_args()
    except:
        sys.exit(WORKER_INIT_FAILED)

    sys.argv[1:] = []
    try:
        worker_cls = utils.load_class(args.worker)
    except ConfigurationError as exc:
        logging.error(
            "Can not load worker class '%s': %s", args.worker, exc)
        sys.exit(WORKER_INIT_FAILED)
    except:
        logging.exception("Can not load worker class: %s", args.worker)
        sys.exit(WORKER_INIT_FAILED)

    if args.app:
        try:
            app_cls = DottedNameResolver().resolve(args.app)
        except ConfigurationError as exc:
            logging.error(
                "Can not load application class '%s': %s", args.app, exc)
            sys.exit(WORKER_INIT_FAILED)
        except:
            logging.exception("Can not load application class: %s", args.app)
            sys.exit(WORKER_INIT_FAILED)
    else:
        app_cls = None

    try:
        worker = worker_cls(app_cls, os.getppid(), args)
        worker._init_process()
    except SystemExit:
        raise
    except ConfigurationError:
        sys.exit(WORKER_INIT_FAILED)
    except BaseException as exc:
        logging.exception("Can not initialize worker %r: %s", worker_cls, exc)
        sys.exit(WORKER_BOOT_FAILED)

    try:
        worker._run()
    except SystemExit:
        raise
    except ConfigurationError:
        sys.exit(WORKER_INIT_FAILED)
    except BaseException as exc:
        logging.exception("Can not run worker: %s", exc)
        sys.exit(WORKER_BOOT_FAILED)
    else:
        sys.exit(0)


if __name__ == '__main__':
    run()
