A Process Control System
========================

fectl is a client/server system that allows its users to monitor and control a number of processes on UNIX-like operating systems.

It is similar to supervisord. Unlike supervisord controlled process has to support fectl and communicate with master with specific protocol.
This gives ability to use custom loading capabilities, worker heartbeats, custom workers communications, etc. Downside of this decision, it is not
possible to run arbitrary process, each application has to support communication protocol.


Configuration
-------------

By default `fectld` uses `fectld.toml` file from current directory. It is possible to override
this by specifing `-c` option. Configuraiton file uses `toml <https://github.com/toml-lang/toml>`_ format.


``[master]`` Section Settings
-----------------------------

The :file:`fectld.toml` file contains a section named
``[master]`` under which configuration parameters for an master process should be inserted.
If the configuration file has no ``[master]`` section, default values will be used. The
allowable configuration values are as follows.


``[master]`` Section Values
~~~~~~~~~~~~~~~~~~~~~~~~~~~

``sock``

  A path to a UNIX domain socket (e.g. :file:`/tmp/fectld.sock`)
  on which fectl will listen for client requests.
  :program:`fectld` uses custom json protocol to communicate with master process
  over this socket.

           *Default*:  fectld.sock

  *Required*:  No.

``directory``

  When :program:`fectld` daemonizes, switch to this directory.

  *Default*: do not cd

  *Required*:  No.


``pid``

   A path to a file where pid of the master process should be
   stored (e.g. :file:`/var/run/fectld.pid`)

   *Default*:  Do not store pid

   *Required*:  No.


``gid``

  Instruct :program:`fectld` to switch groups to this UNIX group
  account before doing any meaningful processing. Value of this
  field could be actual groupd id or group name.

  *Default*: do not switch groups

  *Required*:  No.

``uid``

  Instruct :program:`fectld` to switch users to this UNIX user
  account before doing any meaningful processing. Value of this
  field could be actual user id or user name.

  *Default*: do not switch users

  *Required*:  No.

``stdout``

  A path to a file where `fectld` should redirect stdout.

  *Default*: do not redirect stdout

  *Required*:  No.


``stderr``

  A path to a file where `fectld` should redirect stderr.

  *Default*: do not redirect stderr

  *Required*:  No.


``[[socket]]`` Section Settings
-------------------------------

:program:`fectld` can manage inet sockets for worker processes. i.e. it can open listening socket
and pass file descriptors into work via environment variable. The
allowable configuration values are as follows.

``name``

  A name of the socket. File descriptor is available in worker process as `FECTL_FD_%(name)`
  environment variable.

  *Required*:  Yes.

``port``

  A port number.

  *Required*:  Yes.

``host``

  A host name.

  *Required*:  No.


``backlog``

  The maximum number of pending connections.

  This refers to the number of clients that can be waiting to be served.
  Exceeding this number results in the client getting an error when
  attempting to connect. It should only affect servers under significant
  load.

  Must be a positive integer. Generally set in the 64-2048 range.

  *Default*: 256

  *Required*:  No.


``proto``

  Socket protocol to use. Three options are available *tcp4* - ipv4,
  *tcp6* - ipv7, *unix" - unix domain socket path.

  *Default*: tcp4

  *Required*:  No.


``service``

  List of services that can access this socket.

  *Default*: all services can access socket.

  *Required*:  No.


``app``

  Worker specific setting. Value of the ``app`` field is available as ``FECTL_APP_%(name)``
  environment variable.

  *Required*:  No.

``arguments``

  List of worker specific settings. Value of the ``arguments`` field is available as ``FECTL_ARGS_%(name)``
  environment variable.

  *Required*:  No.

.. note::

   ``app`` and ``arguments`` are used by specific worker. i.e. Python's `asyncio` worker can load `aiohttp` application
   with specific set of arguments.


``[[service]]`` Section Settings
--------------------------------

Each managed application can be configured with ``[[service]]`` section. It is possible to
specify number of workers, various timeouts, and command line. The
allowable configuration values are as follows.


``name``

  A name of the service. This name is used as service identifier, all cammands that can be send
  to service require this name.

  *Required*:  Yes.


``num``

  A number of workers to start. Must be a positive integer.

  *Required*:  Yes.

``command``

  An application start command. ``fectld`` passes configuration (like socket fds, app config, etc)
  in environment variables. Application has to support ``fectl`` communication protocol. ``fectl``
  provides several workers implementation for python, like asyncio and gevent workers.

  *Required*:  Yes.

``directory``

  Before :program:`fectl` executes command, switch to this directory.

  *Default*: do not cd

  *Required*: No.

``restarts``

  Number of restarts before marking worker as failed.

  *Default*:  3

  *Required*:  No.

``gid``

  Switch worker process to run as this group.

  A valid group id (as an integer) or the name of a user that can be
  retrieved with a call to ``libc::getgrnam(value)`` or ``None`` to not
  change the worker processes group. If :program:`fectl` can not change group,
  worker failes to start.

  *Required*:  No.

``uid``

  Switch worker processes to run as this user.
  A valid user id (as an integer) or the name of a user that can be
  retrieved with a call to ``libc::getpwnam(value)`` or ``None`` to not
  change the worker process user. If :program:`fectld` can not change group,
  worker failes to start.

  *Required*:  No.

``timeout``

  Worker has to send `heartbeat` messages to master process. Workers silent for more than this many
  seconds are killed and restarted.

  *Default*: 10

  *Required*: No.

``startup_timeout``

  Timeout for worker startup. After start, workers have this much time to report
  readyness state. Workers that do not report `loaded` state to master are force killed and
  get restarted. After three attempts service marked as failed.

  *Default*: 30

  *Required*: No.

``shutdown_timeout``

  Timeout for graceful workers shutdown. After receiving a restart or stop signal,
  workers have this much time to finish serving requests or any other activity. Workers still alive after
  the timeout (starting from the receipt of the restart signal) are force killed.

  *Default*: 30

  *Required*: No.
