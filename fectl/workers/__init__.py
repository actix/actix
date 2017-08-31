import sys
from enum import Enum

from ..errors import ConfigurationError


class WorkerType(Enum):
    Gevent = 'gevent'
    Asyncio = 'asyncio'


def check_gevent():
    try:
        import gevent
    except:
        raise ConfigurationError("gevent package is not installed")


def check_asyncio():
    if sys.version_info < (3, 4):
        raise ConfigurationError("At least python 3.4 is required")


WORKERS = {
    WorkerType.Gevent.value: (
        "fectl.workers.gevent.GeventWorker", check_gevent),
    WorkerType.Asyncio.value: (
        "fectl.workers.asyncio.AsyncioWorker", check_asyncio),
}
