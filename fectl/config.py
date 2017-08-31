# -*- coding: utf-8 -
#
# This file is part of gunicorn released under the MIT license.
# See the NOTICE for more information.

# Please remember to run "make -C docs html" after update "desc" attributes.

import copy
import grp
import inspect
import os
import pwd
import ssl
import sys
import textwrap
import six

from .errors import ConfigurationError


PLATFORM = sys.platform


def wrap_method(func):
    def _wrapped(instance, *args, **kwargs):
        return func(*args, **kwargs)
    return _wrapped


def auto_int(_, x):
    if x.startswith('0') and not x.lower().startswith('0x'):
        # for compatible with octal numbers in python3
        x = x.replace('0', '0o', 1)
    return int(x, 0)


class Settings(object):

    def __init__(self):
        self.settings = []

    def make_settings(self, arguments, ignore=None):
        settings = {}
        ignore = ignore or ()
        for s in self.settings:
            setting = s()
            if s.name in ignore:
                continue

            if s.name in arguments:
                setting.set(arguments[s.name])

            if setting.default is not None:
                setting.set(setting.default)

            settings[setting.name] = setting.copy()

        return settings

    def add(self, setting_cls):
        setting = setting_cls
        setting.order = len(self.settings)
        setting.validator = wrap_method(setting.validator)

        desc = textwrap.dedent(getattr(setting, 'desc', '')).strip()
        setting.desc = desc
        setting.short = desc.splitlines()[0]

        self.settings.append(setting)


class Setting(object):
    name = None
    value = None
    section = None
    cli = None
    validator = None
    type = None
    meta = None
    action = None
    default = None
    short = None
    desc = None
    nargs = None
    const = None

    def __init__(self):
        if self.default is not None:
            self.set(self.default)

    def add_option(self, parser):
        if not self.cli:
            return
        args = tuple(self.cli)

        help_txt = "%s [%s]" % (self.short, self.default)
        help_txt = help_txt.replace("%", "%%")

        kwargs = {
            "dest": self.name,
            "action": self.action or "store",
            "type": self.type or str,
            "default": None,
            "help": help_txt
        }

        if self.meta is not None:
            kwargs['metavar'] = self.meta

        if kwargs["action"] != "store":
            kwargs.pop("type")

        if self.nargs is not None:
            kwargs["nargs"] = self.nargs

        if self.const is not None:
            kwargs["const"] = self.const

        parser.add_argument(*args, **kwargs)

    def copy(self):
        return copy.copy(self)

    def get(self):
        return self.value

    def set(self, val):
        try:
            if not six.callable(self.validator):
                raise TypeError('Invalid validator: %s' % self.name)
            self.value = self.validator(val)
        except Exception as exc:
            raise ConfigurationError(
                "Can not load config value for '%s': %s" % (self.name, exc))

    def __lt__(self, other):
        return (self.section == other.section and
                self.order < other.order)
    __cmp__ = __lt__


def validate_bool(val):
    if val is None:
        return

    if isinstance(val, bool):
        return val
    if not isinstance(val, six.string_types):
        raise TypeError("Invalid type for casting: %s" % val)
    if val.lower().strip() == "true":
        return True
    elif val.lower().strip() == "false":
        return False
    else:
        raise ValueError("Invalid boolean: %s" % val)


def validate_dict(val):
    if not isinstance(val, dict):
        raise TypeError("Value is not a dictionary: %s " % val)
    return val


def validate_pos_int(val):
    if not isinstance(val, six.integer_types):
        val = int(val, 0)
    else:
        # Booleans are ints!
        val = int(val)
    if val < 0:
        raise ValueError("Value must be positive: %s" % val)
    return val


def validate_string(val):
    if val is None:
        return None
    if not isinstance(val, six.string_types):
        raise TypeError("Not a string: %s" % val)
    return val.strip()


def validate_file_exists(val):
    if val is None:
        return None
    if not os.path.exists(val):
        raise ValueError("File '%s' does not exists." % val)
    return val


def validate_list_string(val):
    if not val:
        return []

    # legacy syntax
    if isinstance(val, six.string_types):
        val = [val]

    return [validate_string(v) for v in val]


def validate_list_of_existing_files(val):
    return [validate_file_exists(v) for v in validate_list_string(val)]


def validate_string_to_list(val):
    val = validate_string(val)

    if not val:
        return []

    return [v.strip() for v in val.split(",") if v]


def validate_class(val):
    if inspect.isfunction(val) or inspect.ismethod(val):
        val = val()
    if inspect.isclass(val):
        return val
    return validate_string(val)


def validate_callable(arity):
    def _validate_callable(val):
        if isinstance(val, six.string_types):
            try:
                mod_name, obj_name = val.rsplit(".", 1)
            except ValueError:
                raise TypeError("Value '%s' is not import string. "
                                "Format: module[.submodules...].object" % val)
            try:
                mod = __import__(mod_name, fromlist=[obj_name])
                val = getattr(mod, obj_name)
            except ImportError as e:
                raise TypeError(str(e))
            except AttributeError:
                raise TypeError(
                    "Can not load '%s' from '%s'" % (obj_name, mod_name))
        if not six.callable(val):
            raise TypeError("Value is not six.callable: %s" % val)
        if arity != -1 and arity != _compat.get_arity(val):
            raise TypeError("Value must have an arity of: %s" % arity)
        return val
    return _validate_callable


def validate_user(val):
    if val is None:
        return os.geteuid()
    if isinstance(val, int):
        return val
    elif val.isdigit():
        return int(val)
    else:
        try:
            return pwd.getpwnam(val).pw_uid
        except KeyError:
            raise ConfigurationError("No such user: '%s'" % val)


def validate_group(val):
    if val is None:
        return os.getegid()

    if isinstance(val, int):
        return val
    elif val.isdigit():
        return int(val)
    else:
        try:
            return grp.getgrnam(val).gr_gid
        except KeyError:
            raise ConfigurationError("No such group: '%s'" % val)


def validate_post_request(val):
    val = validate_callable(-1)(val)

    largs = _compat.get_arity(val)
    if largs == 4:
        return val
    elif largs == 3:
        return lambda worker, req, env, _r: val(worker, req, env)
    elif largs == 2:
        return lambda worker, req, _e, _r: val(worker, req)
    else:
        raise TypeError("Value must have an arity of: 4")


def validate_hostport(val):
    val = validate_string(val)
    if val is None:
        return None
    elements = val.split(":")
    if len(elements) == 2:
        return (elements[0], int(elements[1]))
    else:
        raise TypeError("Value must consist of: hostname:port")


def validate_reload_engine(val):
    if val not in reloader_engines:
        raise ConfigurationError("Invalid reload_engine: %r" % val)

    return val


class ConfigFile(Setting):
    name = "config"
    section = "Config File"
    cli = ["-c", "--config"]
    meta = "CONFIG"
    validator = validate_string
    default = None
    desc = """\
        The fectl config file.

        A string of the form ``PATH``, ``file:PATH``, or ``python:MODULE_NAME``.

        Only has an effect when specified on the command line or as part of an
        application specific configuration.
        """


class MaxRequests(Setting):
    name = "max_requests"
    section = "Worker Processes"
    cli = ["--max-requests"]
    meta = "INT"
    validator = validate_pos_int
    type = int
    default = 0
    desc = """\
        The maximum number of requests a worker will process before restarting.

        Any value greater than zero will limit the number of requests a work
        will process before automatically restarting. This is a simple method
        to help limit the damage of memory leaks.

        If this is set to zero (the default) then the automatic worker
        restarts are disabled.
        """


class SlowRequestTimeout(Setting):
    name = "slow_request_timeout"
    section = "Worker Processes"
    cli = ["--slow-request-timeout"]
    meta = "INT"
    validator = validate_pos_int
    type = int
    default = 0
    desc = """\
        Slow request timeout.

        The maximum duration for request headers reading.
        If this is set to zero (the default) then the timeout is disabled
        """


class MaxRequestsJitter(Setting):
    name = "max_requests_jitter"
    section = "Worker Processes"
    cli = ["--max-requests-jitter"]
    meta = "INT"
    validator = validate_pos_int
    type = int
    default = 0
    desc = """\
        The maximum jitter to add to the *max_requests* setting.

        The jitter causes the restart per worker to be randomized by
        ``randint(0, max_requests_jitter)``. This is intended to stagger worker
        restarts to avoid all workers restarting at the same time.
        """


class GracefulTimeout(Setting):
    name = "graceful_timeout"
    section = "Worker Processes"
    cli = ["--graceful-timeout"]
    meta = "INT"
    validator = validate_pos_int
    type = int
    default = 30
    desc = """\
        Timeout for graceful workers restart.

        After receiving a restart signal, workers have this much time to finish
        serving requests. Workers still alive after the timeout (starting from
        the receipt of the restart signal) are force killed.
        """


class Keepalive(Setting):
    name = "keepalive"
    section = "Worker Processes"
    cli = ["--keep-alive"]
    meta = "INT"
    validator = validate_pos_int
    type = int
    default = 2
    desc = """\
        The number of seconds to wait for requests on a Keep-Alive connection.

        Generally set in the 1-5 seconds range for servers with direct connection
        to the client (e.g. when you don't have separate load balancer). When
        Aiohttp is deployed behind a load balancer, it often makes sense to
        set this to a higher value.
        """


class LimitRequestLine(Setting):
    name = "limit_request_line"
    section = "Security"
    cli = ["--limit-request-line"]
    meta = "INT"
    validator = validate_pos_int
    type = int
    default = 4094
    desc = """\
        The maximum size of HTTP request line in bytes.

        This parameter is used to limit the allowed size of a client's
        HTTP request-line. Since the request-line consists of the HTTP
        method, URI, and protocol version, this directive places a
        restriction on the length of a request-URI allowed for a request
        on the server. A server needs this value to be large enough to
        hold any of its resource names, including any information that
        might be passed in the query part of a GET request. Value is a number
        from 0 (unlimited) to 8190.

        This parameter can be used to prevent any DDOS attack.
        """


class LimitRequestFields(Setting):
    name = "limit_request_fields"
    section = "Security"
    cli = ["--limit-request-fields"]
    meta = "INT"
    validator = validate_pos_int
    type = int
    default = 100
    desc = """\
        Limit the number of HTTP headers fields in a request.

        This parameter is used to limit the number of headers in a request to
        prevent DDOS attack. Used with the *limit_request_field_size* it allows
        more safety. By default this value is 100 and can't be larger than
        32768.
        """


class LimitRequestFieldSize(Setting):
    name = "limit_request_field_size"
    section = "Security"
    cli = ["--limit-request-field_size"]
    meta = "INT"
    validator = validate_pos_int
    type = int
    default = 8190
    desc = """\
        Limit the allowed size of an HTTP request header field.

        Value is a positive number or 0. Setting it to 0 will allow unlimited
        header field sizes.

        .. warning::
           Setting this parameter to a very high or unlimited value can open
           up for DDOS attacks.
        """


class Reload(Setting):
    name = "reload"
    section = 'Debugging'
    cli = ['--reload']
    validator = validate_bool
    action = 'store_true'
    default = False

    desc = '''\
        Restart workers when code changes.

        This setting is intended for development. It will cause workers to be
        restarted whenever application code changes.

        The reloader is incompatible with application preloading. When using a
        paste configuration be sure that the server block does not import any
        application code or the reload will not work as designed.

        The default behavior is to attempt inotify with a fallback to file
        system polling. Generally, inotify should be preferred if available
        because it consumes less system resources.

        .. note::
           In order to use the inotify reloader, you must have the ``inotify``
           package installed.
        '''


class ReloadEngine(Setting):
    name = "reload_engine"
    section = "Debugging"
    cli = ["--reload-engine"]
    meta = "STRING"
    validator = validate_reload_engine
    default = "auto"
    desc = """\
        The implementation that should be used to power :ref:`reload`.

        Valid engines are:

        * 'auto'
        * 'poll'
        * 'inotify' (requires inotify)

        .. versionadded:: 19.7
        """


class ReloadExtraFiles(Setting):
    name = "reload_extra_files"
    action = "append"
    section = "Debugging"
    cli = ["--reload-extra-file"]
    meta = "FILES"
    validator = validate_list_of_existing_files
    default = []
    desc = """\
        Extends :ref:`reload` option to also watch and reload on additional files
        (e.g., templates, configurations, specifications, etc.).

        .. versionadded:: 19.8
        """


class Spew(Setting):
    name = "spew"
    section = "Debugging"
    cli = ["--spew"]
    validator = validate_bool
    action = "store_true"
    default = False
    desc = """\
        Install a trace function that spews every line executed by the server.

        This is the nuclear option.
        """


class ConfigCheck(Setting):
    name = "check_config"
    section = "Debugging"
    cli = ["--check-config"]
    validator = validate_bool
    action = "store_true"
    default = False
    desc = """\
        Check the configuration.
        """


class Sendfile(Setting):
    name = "sendfile"
    section = "Server Mechanics"
    cli = ["--no-sendfile"]
    validator = validate_bool
    action = "store_const"
    const = False

    desc = """\
        Disables the use of ``sendfile()``.

        If not set, the value of the ``SENDFILE`` environment variable is used
        to enable or disable its usage.
        """


class Pidfile(Setting):
    name = "pidfile"
    section = "Server Mechanics"
    cli = ["-p", "--pid"]
    meta = "FILE"
    validator = validate_string
    default = None
    desc = """\
        A filename to use for the PID file.

        If not set, no PID file will be written.
        """


class WorkerTmpDir(Setting):
    name = "worker_tmp_dir"
    section = "Server Mechanics"
    cli = ["--worker-tmp-dir"]
    meta = "DIR"
    validator = validate_string
    default = None
    desc = """\
        A directory to use for the worker heartbeat temporary file.

        If not set, the default temporary directory will be used.

        .. note::
           The current heartbeat system involves calling ``os.fchmod`` on
           temporary file handlers and may block a worker for arbitrary time
           if the directory is on a disk-backed filesystem.

           See :ref:`blocking-os-fchmod` for more detailed information
           and a solution for avoiding this problem.
        """


class Umask(Setting):
    name = "umask"
    section = "Server Mechanics"
    cli = ["-m", "--umask"]
    meta = "INT"
    validator = validate_pos_int
    type = auto_int
    default = 0
    desc = """\
        A bit mask for the file mode on files written by FECTL.

        Note that this affects unix socket permissions.

        A valid value for the ``os.umask(mode)`` call or a string compatible
        with ``int(value, 0)`` (``0`` means Python guesses the base, so values
        like ``0``, ``0xFF``, ``0022`` are valid for decimal, hex, and octal
        representations)
        """


class Initgroups(Setting):
    name = "initgroups"
    section = "Server Mechanics"
    cli = ["--initgroups"]
    validator = validate_bool
    action = 'store_true'
    default = False

    desc = """\
        If true, set the worker process's group access list with all of the
        groups of which the specified username is a member, plus the specified
        group id.

        .. versionadded:: 19.7
        """


class SecureSchemeHeader(Setting):
    name = "secure_scheme_headers"
    section = "Server Mechanics"
    validator = validate_dict
    default = {
        "X-FORWARDED-PROTOCOL": "ssl",
        "X-FORWARDED-PROTO": "https",
        "X-FORWARDED-SSL": "on"
    }
    desc = """\

        A dictionary containing headers and values that the front-end proxy
        uses to indicate HTTPS requests. These tell http server to set
        ``wsgi.url_scheme`` to ``https``, so your application can tell that the
        request is secure.

        The dictionary should map upper-case header names to exact string
        values. The value comparisons are case-sensitive, unlike the header
        names, so make sure they're exactly what your front-end proxy sends
        when handling HTTPS requests.

        It is important that your front-end proxy configuration ensures that
        the headers defined here can not be passed directly from the client.
        """


class ForwardedAllowIPS(Setting):
    name = "forwarded_allow_ips"
    section = "Server Mechanics"
    cli = ["--forwarded-allow-ips"]
    meta = "STRING"
    validator = validate_string_to_list
    default = os.environ.get("FORWARDED_ALLOW_IPS", "127.0.0.1")
    desc = """\
        Front-end's IPs from which allowed to handle set secure headers.
        (comma separate).

        Set to ``*`` to disable checking of Front-end IPs (useful for setups
        where you don't know in advance the IP address of Front-end, but
        you still trust the environment).

        By default, the value of the ``FORWARDED_ALLOW_IPS`` environment
        variable. If it is not defined, the default is ``"127.0.0.1"``.
        """


class AccessLog(Setting):
    name = "accesslog"
    section = "Logging"
    cli = ["--access-logfile"]
    meta = "FILE"
    validator = validate_string
    default = None
    desc = """\
        The Access log file to write to.

        ``'-'`` means log to stdout.
        """


class DisableRedirectAccessToSyslog(Setting):
    name = "disable_redirect_access_to_syslog"
    section = "Logging"
    cli = ["--disable-redirect-access-to-syslog"]
    validator = validate_bool
    action = 'store_true'
    default = False
    desc = """\
    Disable redirect access logs to syslog.

    .. versionadded:: 19.8
    """


class AccessLogFormat(Setting):
    name = "access_log_format"
    section = "Logging"
    cli = ["--access-logformat"]
    meta = "STRING"
    validator = validate_string
    default = '%(h)s %(l)s %(u)s %(t)s "%(r)s" %(s)s %(b)s "%(f)s" "%(a)s"'
    desc = """\
        The access log format.

        ===========  ===========
        Identifier   Description
        ===========  ===========
        h            remote address
        l            ``'-'``
        u            user name
        t            date of the request
        r            status line (e.g. ``GET / HTTP/1.1``)
        m            request method
        U            URL path without query string
        q            query string
        H            protocol
        s            status
        B            response length
        b            response length or ``'-'`` (CLF format)
        f            referer
        a            user agent
        T            request time in seconds
        D            request time in microseconds
        L            request time in decimal seconds
        p            process ID
        {Header}i    request header
        {Header}o    response header
        {Variable}e  environment variable
        ===========  ===========
        """


class ErrorLog(Setting):
    name = "errorlog"
    section = "Logging"
    cli = ["--error-logfile", "--log-file"]
    meta = "FILE"
    validator = validate_string
    default = '-'
    desc = """\
        The Error log file to write to.

        Using ``'-'`` for FILE makes log to stderr.
        """


class Loglevel(Setting):
    name = "loglevel"
    section = "Logging"
    cli = ["--log-level"]
    meta = "LEVEL"
    validator = validate_string
    default = "info"
    desc = """\
        The granularity of Error log outputs.

        Valid level names are:

        * debug
        * info
        * warning
        * error
        * critical
        """


class CaptureOutput(Setting):
    name = "capture_output"
    section = "Logging"
    cli = ["--capture-output"]
    validator = validate_bool
    action = 'store_true'
    default = False
    desc = """\
        Redirect stdout/stderr to Error log.
        """


class LoggerClass(Setting):
    name = "logger_class"
    section = "Logging"
    cli = ["--logger-class"]
    meta = "STRING"
    validator = validate_class
    default = "gunicorn.glogging.Logger"
    desc = """\
        The logger you want to use to log events in FECTL.

        The default class (``gunicorn.glogging.Logger``) handle most of
        normal usages in logging. It provides error and access logging.

        You can provide your own logger by giving Gunicorn a
        Python path to a subclass like ``gunicorn.glogging.Logger``.
        """


class LogConfig(Setting):
    name = "logconfig"
    section = "Logging"
    cli = ["--log-config"]
    meta = "FILE"
    validator = validate_file_exists
    default = None
    desc = """\
    The log config file to use.
    FECTL uses the standard Python logging module's Configuration
    file format.
    """


class SyslogTo(Setting):
    name = "syslog_addr"
    section = "Logging"
    cli = ["--log-syslog-to"]
    meta = "SYSLOG_ADDR"
    validator = validate_string

    if PLATFORM == "darwin":
        default = "unix:///var/run/syslog"
    elif PLATFORM in ('freebsd', 'dragonfly', ):
        default = "unix:///var/run/log"
    elif PLATFORM == "openbsd":
        default = "unix:///dev/log"
    else:
        default = "udp://localhost:514"

    desc = """\
    Address to send syslog messages.

    Address is a string of the form:

    * ``unix://PATH#TYPE`` : for unix domain socket. ``TYPE`` can be ``stream``
      for the stream driver or ``dgram`` for the dgram driver.
      ``stream`` is the default.
    * ``udp://HOST:PORT`` : for UDP sockets
    * ``tcp://HOST:PORT`` : for TCP sockets

    """


class Procname(Setting):
    name = "proc_name"
    section = "Process Naming"
    cli = ["-n", "--name"]
    meta = "STRING"
    validator = validate_string
    default = None
    desc = """\
        A base to use with setproctitle for process naming.

        This affects things like ``ps`` and ``top``. If you're going to be
        running more than one instance of FECTL you'll probably want to set a
        name to tell them apart. This requires that you install the setproctitle
        module.

        If not set, the *default_proc_name* setting will be used.
        """


class DefaultProcName(Setting):
    name = "default_proc_name"
    section = "Process Naming"
    validator = validate_string
    default = "fectl"
    desc = """\
        Internal setting that is adjusted for each type of application.
        """


class PythonPath(Setting):
    name = "pythonpath"
    section = "Server Mechanics"
    cli = ["--pythonpath"]
    meta = "STRING"
    validator = validate_string
    default = None
    desc = """\
        A comma-separated list of directories to add to the Python path.

        e.g.
        ``'/home/djangoprojects/myproject,/home/python/mylibrary'``.
        """


class Paste(Setting):
    name = "paste"
    section = "Server Mechanics"
    cli = ["--paste", "--paster"]
    meta = "STRING"
    validator = validate_string
    default = None
    desc = """\
        Load a PasteDeploy config file. The argument may contain a ``#``
        symbol followed by the name of an app section from the config file,
        e.g. ``production.ini#admin``.

        At this time, using alternate server blocks is not supported. Use the
        command line arguments to control server configuration instead.
        """


class OnStarting(Setting):
    name = "on_starting"
    section = "Server Hooks"
    validator = validate_callable(1)
    type = six.callable

    def on_starting(server):
        pass
    default = staticmethod(on_starting)
    desc = """\
        Called just before the master process is initialized.

        The callable needs to accept a single instance variable for the Arbiter.
        """


class OnReload(Setting):
    name = "on_reload"
    section = "Server Hooks"
    validator = validate_callable(1)
    type = six.callable

    def on_reload(server):
        pass
    default = staticmethod(on_reload)
    desc = """\
        Called to recycle workers during a reload via SIGHUP.

        The callable needs to accept a single instance variable for the Arbiter.
        """


class WhenReady(Setting):
    name = "when_ready"
    section = "Server Hooks"
    validator = validate_callable(1)
    type = six.callable

    def when_ready(server):
        pass
    default = staticmethod(when_ready)
    desc = """\
        Called just after the server is started.

        The callable needs to accept a single instance variable for the Arbiter.
        """


class PostWorkerInit(Setting):
    name = "post_worker_init"
    section = "Server Hooks"
    validator = validate_callable(1)
    type = six.callable

    def post_worker_init(worker):
        pass

    default = staticmethod(post_worker_init)
    desc = """\
        Called just after a worker has initialized the application.

        The callable needs to accept one instance variable for the initialized
        Worker.
        """


class WorkerInt(Setting):
    name = "worker_int"
    section = "Server Hooks"
    validator = validate_callable(1)
    type = six.callable

    def worker_int(worker):
        pass

    default = staticmethod(worker_int)
    desc = """\
        Called just after a worker exited on SIGINT or SIGQUIT.

        The callable needs to accept one instance variable for the initialized
        Worker.
        """


class WorkerAbort(Setting):
    name = "worker_abort"
    section = "Server Hooks"
    validator = validate_callable(1)
    type = six.callable

    def worker_abort(worker):
        pass

    default = staticmethod(worker_abort)
    desc = """\
        Called when a worker received the SIGABRT signal.

        This call generally happens on timeout.

        The callable needs to accept one instance variable for the initialized
        Worker.
        """


class PreRequest(Setting):
    name = "pre_request"
    section = "Server Hooks"
    validator = validate_callable(2)
    type = six.callable

    def pre_request(worker, req):
        worker.log.debug("%s %s" % (req.method, req.path))
    default = staticmethod(pre_request)
    desc = """\
        Called just before a worker processes the request.

        The callable needs to accept two instance variables for the Worker and
        the Request.
        """


class PostRequest(Setting):
    name = "post_request"
    section = "Server Hooks"
    validator = validate_post_request
    type = six.callable

    def post_request(worker, req, environ, resp):
        pass
    default = staticmethod(post_request)
    desc = """\
        Called after a worker processes the request.

        The callable needs to accept two instance variables for the Worker and
        the Request.
        """


class WorkerExit(Setting):
    name = "worker_exit"
    section = "Server Hooks"
    validator = validate_callable(2)
    type = six.callable

    def worker_exit(server, worker):
        pass
    default = staticmethod(worker_exit)
    desc = """\
        Called just after a worker has been exited, in the worker process.

        The callable needs to accept two instance variables for the Arbiter and
        the just-exited Worker.
        """


class ProxyProtocol(Setting):
    name = "proxy_protocol"
    section = "Server Mechanics"
    cli = ["--proxy-protocol"]
    validator = validate_bool
    default = False
    action = "store_true"
    desc = """\
        Enable detect PROXY protocol (PROXY mode).

        Allow using HTTP and Proxy together. It may be useful for work with
        stunnel as HTTPS frontend and %s as HTTP server.

        PROXY protocol: http://haproxy.1wt.eu/download/1.5/doc/proxy-protocol.txt

        Example for stunnel config::

            [https]
            protocol = proxy
            accept  = 443
            connect = 80
            cert = /etc/ssl/certs/stunnel.pem
            key = /etc/ssl/certs/stunnel.key
        """


class ProxyAllowFrom(Setting):
    name = "proxy_allow_ips"
    section = "Server Mechanics"
    cli = ["--proxy-allow-from"]
    validator = validate_string_to_list
    default = "127.0.0.1"
    desc = """\
    Front-end's IPs from which allowed accept proxy requests (comma separate).

    Set to ``*`` to disable checking of Front-end IPs (useful for setups
    where you don't know in advance the IP address of Front-end, but
    you still trust the environment)
    """


class KeyFile(Setting):
    name = "keyfile"
    section = "SSL"
    cli = ["--keyfile"]
    meta = "FILE"
    validator = validate_file_exists
    default = None
    desc = """\
    SSL key file
    """


class CertFile(Setting):
    name = "certfile"
    section = "SSL"
    cli = ["--certfile"]
    meta = "FILE"
    validator = validate_file_exists
    default = None
    desc = """\
    SSL certificate file
    """


class SSLVersion(Setting):
    name = "ssl_version"
    section = "SSL"
    cli = ["--ssl-version"]
    validator = validate_pos_int
    default = ssl.PROTOCOL_SSLv23
    desc = """\
    SSL version to use (see stdlib ssl module's)
    """


class CertReqs(Setting):
    name = "cert_reqs"
    section = "SSL"
    cli = ["--cert-reqs"]
    validator = validate_pos_int
    default = ssl.CERT_NONE
    desc = """\
    Whether client certificate is required (see stdlib ssl module's)
    """


class CACerts(Setting):
    name = "ca_certs"
    section = "SSL"
    cli = ["--ca-certs"]
    meta = "FILE"
    validator = validate_file_exists
    default = None
    desc = """\
    CA certificates file
    """


class SuppressRaggedEOFs(Setting):
    name = "suppress_ragged_eofs"
    section = "SSL"
    cli = ["--suppress-ragged-eofs"]
    action = "store_true"
    default = True
    validator = validate_bool
    desc = """\
    Suppress ragged EOFs (see stdlib ssl module's)
    """


class DoHandshakeOnConnect(Setting):
    name = "do_handshake_on_connect"
    section = "SSL"
    cli = ["--do-handshake-on-connect"]
    validator = validate_bool
    action = "store_true"
    default = False
    desc = """\
    Whether to perform SSL handshake on socket connect (see stdlib ssl module's)
    """


class Ciphers(Setting):
    name = "ciphers"
    section = "SSL"
    cli = ["--ciphers"]
    validator = validate_string
    default = 'TLSv1'
    desc = """\
    Ciphers to use (see stdlib ssl module's)
    """
