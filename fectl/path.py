"""Copy of pyramid's path resolver,
https://github.com/Pylons/pyramid/blob/master/pyramid/path.py """

import imp
import os
import pkg_resources
import six
import sys


ignore_types = [imp.C_EXTENSION, imp.C_BUILTIN]
init_names = ['__init__%s' % x[0] for x in imp.get_suffixes() if
              x[0] and x[2] not in ignore_types]


def caller_path(path, level=2):
    if not os.path.isabs(path):
        module = caller_module(level + 1)
        prefix = package_path(module)
        path = os.path.join(prefix, path)
    return path


def caller_module(level=2, sys=sys):
    module_globals = sys._getframe(level).f_globals
    module_name = module_globals.get('__name__') or '__main__'
    module = sys.modules[module_name]
    return module


def package_name(pkg_or_module):
    """ If this function is passed a module, return the dotted Python
    package name of the package in which the module lives.  If this
    function is passed a package, return the dotted Python package
    name of the package itself."""
    if pkg_or_module is None or pkg_or_module.__name__ == '__main__':
        return '__main__'
    pkg_name = pkg_or_module.__name__
    pkg_filename = getattr(pkg_or_module, '__file__', None)
    if pkg_filename is None:
        # Namespace packages do not have __init__.py* files,
        # and so have no __file__ attribute
        return pkg_name
    splitted = os.path.split(pkg_filename)
    if splitted[-1] in init_names:
        # it's a package
        return pkg_name
    return pkg_name.rsplit('.', 1)[0]


def package_of(pkg_or_module):
    """ Return the package of a module or return the package itself """
    pkg_name = package_name(pkg_or_module)
    __import__(pkg_name)
    return sys.modules[pkg_name]


def caller_package(level=2, caller_module=caller_module):
    # caller_module in arglist for tests
    module = caller_module(level + 1)
    f = getattr(module, '__file__', '')
    if (('__init__.py' in f) or ('__init__$py' in f)):  # empty at >>>
        # Module is a package
        return module
    # Go up one level to get package
    package_name = module.__name__.rsplit('.', 1)[0]
    return sys.modules[package_name]


def package_path(package):
    # computing the abspath is actually kinda expensive so we memoize
    # the result
    prefix = getattr(package, '__abspath__', None)
    if prefix is None:
        prefix = pkg_resources.resource_filename(package.__name__, '')
        # pkg_resources doesn't care whether we feed it a package
        # name or a module name within the package, the result
        # will be the same: a directory name to the package itself
        try:
            package.__abspath__ = prefix
        except:
            # this is only an optimization, ignore any error
            pass
    return prefix


class _CALLER_PACKAGE(object):
    def __repr__(self):  # pragma: no cover (for docs)
        return 'fectl.path.CALLER_PACKAGE'


CALLER_PACKAGE = _CALLER_PACKAGE()


class Resolver(object):
    def __init__(self, package=CALLER_PACKAGE):
        if package in (None, CALLER_PACKAGE):
            self.package = package
        else:
            if isinstance(package, six.string_types):
                try:
                    __import__(package)
                except ImportError:
                    raise ValueError(
                        'The dotted name %r cannot be imported' % (package,)
                    )
                package = sys.modules[package]
            self.package = package_of(package)

    def get_package_name(self):
        if self.package is CALLER_PACKAGE:
            package_name = caller_package().__name__
        else:
            package_name = self.package.__name__
        return package_name

    def get_package(self):
        if self.package is CALLER_PACKAGE:
            package = caller_package()
        else:
            package = self.package
        return package


class DottedNameResolver(Resolver):
    """ A class used to resolve a :term:`dotted Python name` to a package or
    module object.

    .. versionadded:: 1.3

    The constructor accepts a single argument named ``package`` which may be
    any of:

    - A fully qualified (not relative) dotted name to a module or package

    - a Python module or package object

    - The value ``None``

    - The constant value :attr:`fectl.path.CALLER_PACKAGE`.

    The default value is :attr:`fectl.path.CALLER_PACKAGE`.

    The ``package`` is used when a relative dotted name is supplied to the
    :meth:`~fectl.path.DottedNameResolver.resolve` method.  A dotted name
    which has a ``.`` (dot) or ``:`` (colon) as its first character is
    treated as relative.

    If ``package`` is ``None``, the resolver will only be able to resolve
    fully qualified (not relative) names.  Any attempt to resolve a
    relative name will result in an :exc:`ValueError` exception.

    If ``package`` is :attr:`fectl.path.CALLER_PACKAGE`,
    the resolver will treat relative dotted names as relative to
    the caller of the :meth:`~fectl.path.DottedNameResolver.resolve`
    method.

    If ``package`` is a *module* or *module name* (as opposed to a package or
    package name), its containing package is computed and this
    package used to derive the package name (all names are resolved relative
    to packages, never to modules).  For example, if the ``package`` argument
    to this type was passed the string ``xml.dom.expatbuilder``, and
    ``.mindom`` is supplied to the
    :meth:`~fectl.path.DottedNameResolver.resolve` method, the resulting
    import would be for ``xml.minidom``, because ``xml.dom.expatbuilder`` is
    a module object, not a package object.

    If ``package`` is a *package* or *package name* (as opposed to a module or
    module name), this package will be used to relative compute
    dotted names.  For example, if the ``package`` argument to this type was
    passed the string ``xml.dom``, and ``.minidom`` is supplied to the
    :meth:`~fectl.path.DottedNameResolver.resolve` method, the resulting
    import would be for ``xml.minidom``.
    """
    def resolve(self, dotted):
        """
        This method resolves a dotted name reference to a global Python
        object (an object which can be imported) to the object itself.

        Two dotted name styles are supported:

        - ``pkg_resources``-style dotted names where non-module attributes
          of a package are separated from the rest of the path using a ``:``
          e.g. ``package.module:attr``.

        - ``zope.dottedname``-style dotted names where non-module
          attributes of a package are separated from the rest of the path
          using a ``.`` e.g. ``package.module.attr``.

        These styles can be used interchangeably.  If the supplied name
        contains a ``:`` (colon), the ``pkg_resources`` resolution
        mechanism will be chosen, otherwise the ``zope.dottedname``
        resolution mechanism will be chosen.

        If the ``dotted`` argument passed to this method is not a string, a
        :exc:`ValueError` will be raised.

        When a dotted name cannot be resolved, a :exc:`ValueError` error is
        raised.

        Example:

        .. code-block:: python

           r = DottedNameResolver()
           v = r.resolve('xml') # v is the xml module

        """
        if not isinstance(dotted, six.string_types):
            raise ValueError('%r is not a string' % (dotted,))
        package = self.package
        if package is CALLER_PACKAGE:
            package = caller_package()
        return self._resolve(dotted, package)

    def maybe_resolve(self, dotted):
        """
        This method behaves just like
        :meth:`~fectl.path.DottedNameResolver.resolve`, except if the
        ``dotted`` value passed is not a string, it is simply returned.  For
        example:

        .. code-block:: python

           import xml
           r = DottedNameResolver()
           v = r.maybe_resolve(xml)
           # v is the xml module; no exception raised
        """
        if isinstance(dotted, six.string_types):
            package = self.package
            if package is CALLER_PACKAGE:
                package = caller_package()
            return self._resolve(dotted, package)
        return dotted

    def _resolve(self, dotted, package):
        if ':' in dotted:
            return self._pkg_resources_style(dotted, package)
        else:
            return self._zope_dottedname_style(dotted, package)

    def _pkg_resources_style(self, value, package):
        """ package.module:attr style """
        if value.startswith(('.', ':')):
            if not package:
                raise ValueError(
                    'relative name %r irresolveable without package' % (value,)
                )
            if value in ['.', ':']:
                value = package.__name__
            else:
                value = package.__name__ + value
        # Calling EntryPoint.load with an argument is deprecated.
        # See https://pythonhosted.org/setuptools/history.html#id8
        ep = pkg_resources.EntryPoint.parse('x=%s' % value)
        if hasattr(ep, 'resolve'):
            # setuptools>=10.2
            return ep.resolve()  # pragma: NO COVER
        else:
            return ep.load(False)  # pragma: NO COVER

    def _zope_dottedname_style(self, value, package):
        """ package.module.attr style """
        module = getattr(package, '__name__', None)  # package may be None
        if not module:
            module = None
        if value == '.':
            if module is None:
                raise ValueError(
                    'relative name %r irresolveable without package' % (value,)
                )
            name = module.split('.')
        else:
            name = value.split('.')
            if not name[0]:
                if module is None:
                    raise ValueError(
                        'relative name %r irresolveable without '
                        'package' % (value,)
                    )
                module = module.split('.')
                name.pop(0)
                while not name[0]:
                    module.pop()
                    name.pop(0)
                name = module + name

        used = name.pop(0)
        found = __import__(used)
        for n in name:
            used += '.' + n
            try:
                found = getattr(found, n)
            except AttributeError:
                __import__(used)
                found = getattr(found, n)  # pragma: no cover

        return found


class AssetResolver(Resolver):
    """ A class used to resolve an :term:`asset specification` to an
    :term:`asset descriptor`.

    .. versionadded:: 1.3

    The constructor accepts a single argument named ``package`` which may be
    any of:

    - A fully qualified (not relative) dotted name to a module or package

    - a Python module or package object

    - The value ``None``

    - The constant value :attr:`fectl.path.CALLER_PACKAGE`.

    The default value is :attr:`fectl.path.CALLER_PACKAGE`.

    The ``package`` is used when a relative asset specification is supplied
    to the :meth:`~fectl.path.AssetResolver.resolve` method.  An asset
    specification without a colon in it is treated as relative.

    If ``package`` is ``None``, the resolver will
    only be able to resolve fully qualified (not relative) asset
    specifications.  Any attempt to resolve a relative asset specification
    will result in an :exc:`ValueError` exception.

    If ``package`` is :attr:`fectl.path.CALLER_PACKAGE`,
    the resolver will treat relative asset specifications as
    relative to the caller of the :meth:`~fectl.path.AssetResolver.resolve`
    method.

    If ``package`` is a *module* or *module name* (as opposed to a package or
    package name), its containing package is computed and this
    package is used to derive the package name (all names are resolved relative
    to packages, never to modules).  For example, if the ``package`` argument
    to this type was passed the string ``xml.dom.expatbuilder``, and
    ``template.pt`` is supplied to the
    :meth:`~fectl.path.AssetResolver.resolve` method, the resulting absolute
    asset spec would be ``xml.minidom:template.pt``, because
    ``xml.dom.expatbuilder`` is a module object, not a package object.

    If ``package`` is a *package* or *package name* (as opposed to a module or
    module name), this package will be used to compute relative
    asset specifications.  For example, if the ``package`` argument to this
    type was passed the string ``xml.dom``, and ``template.pt`` is supplied
    to the :meth:`~fectl.path.AssetResolver.resolve` method, the resulting
    absolute asset spec would be ``xml.minidom:template.pt``.
    """
    def resolve(self, spec):
        """
        Resolve the asset spec named as ``spec`` to an object that has the
        attributes and methods described in
        :class:`fectl.interfaces.IAssetDescriptor`.

        If ``spec`` is an absolute filename
        (e.g. ``/path/to/myproject/templates/foo.pt``) or an absolute asset
        spec (e.g. ``myproject:templates.foo.pt``), an asset descriptor is
        returned without taking into account the ``package`` passed to this
        class' constructor.

        If ``spec`` is a *relative* asset specification (an asset
        specification without a ``:`` in it, e.g. ``templates/foo.pt``), the
        ``package`` argument of the constructor is used as the package
        portion of the asset spec.  For example:

        .. code-block:: python

           a = AssetResolver('myproject')
           resolver = a.resolve('templates/foo.pt')
           print(resolver.abspath())
           # -> /path/to/myproject/templates/foo.pt

        If the AssetResolver is constructed without a ``package`` argument of
        ``None``, and a relative asset specification is passed to
        ``resolve``, an :exc:`ValueError` exception is raised.
        """
        if os.path.isabs(spec):
            return FSAssetDescriptor(spec)
        path = spec
        if ':' in path:
            package_name, path = spec.split(':', 1)
        else:
            if self.package is CALLER_PACKAGE:
                package_name = caller_package().__name__
            else:
                package_name = getattr(self.package, '__name__', None)
            if package_name is None:
                raise ValueError(
                    'relative spec %r irresolveable without package' % (spec,)
                )
        return PkgResourcesAssetDescriptor(package_name, path)


class PkgResourcesAssetDescriptor(object):
    pkg_resources = pkg_resources

    def __init__(self, pkg_name, path):
        self.pkg_name = pkg_name
        self.path = path

    def absspec(self):
        return '%s:%s' % (self.pkg_name, self.path)

    def abspath(self):
        return os.path.abspath(
            self.pkg_resources.resource_filename(self.pkg_name, self.path))

    def stream(self):
        return self.pkg_resources.resource_stream(self.pkg_name, self.path)

    def isdir(self):
        return self.pkg_resources.resource_isdir(self.pkg_name, self.path)

    def listdir(self):
        return self.pkg_resources.resource_listdir(self.pkg_name, self.path)

    def exists(self):
        return self.pkg_resources.resource_exists(self.pkg_name, self.path)


class FSAssetDescriptor(object):

    def __init__(self, path):
        self.path = os.path.abspath(path)

    def absspec(self):
        raise NotImplementedError

    def abspath(self):
        return self.path

    def stream(self):
        return open(self.path, 'rb')

    def isdir(self):
        return os.path.isdir(self.path)

    def listdir(self):
        return os.listdir(self.path)

    def exists(self):
        return os.path.exists(self.path)
