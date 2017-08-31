from .. import config
from ..path import DottedNameResolver


class App(config.Setting):
    name = "app"
    section = "Application"
    cli = ["--app"]
    meta = "STRING"
    desc = """\
        web.Application instance.

        Callable that returns web.Application instance or
        python path to web.Application instance.
        """

    @staticmethod
    def validator(val):
        import aiohttp
        try:
            app = DottedNameResolver().resolve(val)
            if callable(app) or isinstance(app, aiohttp.web.Application):
                return app
        except:
            pass

        raise config.ConfigurationError(
            "Can not load aiohttp application: %s", val)


class AccessLogFormat(config.Setting):
    name = "access_log_format"
    section = "Logging"
    cli = ["--access-logformat"]
    meta = "STRING"
    validator = config.validate_string
    default = '%a %l %u %t "%r" %s %b "%{Referrer}i" "%{User-Agent}i"'
    desc = """\
        The access log format.

        ===========  ===========
        Identifier   Description
        ===========  ===========
        %%  The percent sign
        %a  Remote IP-address (IP-address of proxy if using reverse proxy)
        %t  Time when the request was started to process
        %P  The process ID of the child that serviced the request
        %r  First line of request
        %s  Response status code
        %b  Size of response in bytes, including HTTP headers
        %T  Time taken to serve the request, in seconds
        %Tf Time taken to serve the request, in seconds with floating fraction
            in .06f format
        %D  Time taken to serve the request, in microseconds
        %{FOO}i  request.headers['FOO']
        %{FOO}o  response.headers['FOO']
        %{FOO}e  os.environ['FOO']
        ===========  ===========
        """


class AiohttpSettings(config.Settings):

    def __init__(self):
        super(AiohttpSettings, self).__init__()

        self.add(App)
        self.add(config.GracefulTimeout)
        self.add(config.Keepalive)
        self.add(config.SlowRequestTimeout)
        self.add(config.LimitRequestLine)
        self.add(config.LimitRequestFields)
        self.add(config.LimitRequestFieldSize)
        self.add(config.AccessLog)
        self.add(AccessLogFormat)
        self.add(config.ErrorLog)
        self.add(config.Loglevel)
        self.add(config.CaptureOutput)
        self.add(config.LogConfig)
        self.add(config.Procname)
        self.add(config.DefaultProcName)
        self.add(config.KeyFile)
        self.add(config.CertFile)
        self.add(config.SSLVersion)
        self.add(config.CertReqs)
        self.add(config.CACerts)
        self.add(config.Ciphers)

    def __call__(self, arguments):
        return AiohttpConfig(self.make_settings(arguments))


class AiohttpConfig(object):

    def __init__(self, settings, arguments=None):
        self.settings = settings
        self.arguments = arguments

    def __getattr__(self, name):
        if name not in self.settings:
            raise AttributeError("No configuration setting for: %s" % name)
        return self.settings[name].get()

    def __setattr__(self, name, value):
        if name != "settings" and name in self.settings:
            raise AttributeError("Invalid access!")
        super(AiohttpConfig, self).__setattr__(name, value)

    def set(self, name, value):
        if name not in self.settings:
            raise AttributeError("No configuration setting for: %s" % name)
        self.settings[name].set(value)

    @property
    def proc_name(self):
        pn = self.settings['proc_name'].get()
        if pn is not None:
            return pn
        else:
            return self.settings['default_proc_name'].get()

    # @property
    # def logger_class(self):
    # uri = self.settings['logger_class'].get()
    # if uri == "simple":
    #    # support the default
    #    uri = LoggerClass.default

        # logger_class = util.load_class(
        #    uri,
        #    default="gunicorn.glogging.Logger",
        #    section="gunicorn.loggers")

        # if hasattr(logger_class, "install"):
        #    logger_class.install()
        # return logger_class

    @property
    def is_ssl(self):
        return self.certfile or self.keyfile

    @property
    def ssl_options(self):
        opts = {}
        for name, value in self.settings.items():
            if value.section == 'SSL':
                opts[name] = value.get()
        return opts
