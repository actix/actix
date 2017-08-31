

class ConfigurationError(Exception):
    """ Exception raised on config error """


class UnsupportedWorker(ConfigurationError):
    """ Worker is not supported by application """
