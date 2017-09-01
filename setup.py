from __future__ import print_function
import codecs
import re
import sys
import os.path
import subprocess
from setuptools import setup
from setuptools_rust import RustExtension, Binding, Strip, build_ext
from setuptools.command.test import test as TestCommand
from distutils.errors import (
    DistutilsPlatformError, DistutilsExecError, CCompilerError)


class BuildFailed(Exception):
    pass


class ve_build_ext(build_ext):
    # This class allows C extension building to fail.

    def run(self):
        build_ext.run(self)

    def build_extension(self, ext):
        try:
            build_ext.build_extension(self, ext)
        except (CCompilerError, DistutilsExecError,
                DistutilsPlatformError, ValueError):
            raise BuildFailed()


with codecs.open(os.path.join(os.path.abspath(os.path.dirname(
        __file__)), 'fectl', '__init__.py'), 'r', 'latin1') as fp:
    try:
        version = re.findall(r"^__version__ = '([^']+)'\r?$",
                             fp.read(), re.M)[0]
    except IndexError:
        raise RuntimeError('Unable to determine version.')


class PyTest(TestCommand):
    user_options = []

    def run(self):
        import subprocess
        import sys
        errno = subprocess.call([sys.executable, '-m', 'pytest', 'tests'])
        raise SystemExit(errno)


install_requires = ['six']
tests_require = install_requires + ['pytest', 'pytest-timeout']

if sys.version_info < (3, 4):
    install_requires.append('enum')


def read(f):
    return open(os.path.join(os.path.dirname(__file__), f)).read().strip()


setup_args = dict(
    name='fectl',
    version=version,
    description='Simple process manager',
    long_description='\n\n'.join((read('README.rst'), read('CHANGES.rst'))),
    classifiers=[
        'License :: OSI Approved :: Apache Software License',
        'Intended Audience :: Developers',
        'Programming Language :: Python',
        'Programming Language :: Python :: 2.7',
        'Programming Language :: Python :: 3',
        'Programming Language :: Python :: 3.4',
        'Programming Language :: Python :: 3.5',
        'Programming Language :: Python :: 3.6',
        'Development Status :: 5 - Production/Stable',
        'Operating System :: POSIX',
        'Operating System :: MacOS :: MacOS X',
    ],
    author='Nikolay Kim',
    author_email='fafhrd91@gmail.com',
    url='https://github.com/fafhrd91/fectl/',
    license='Apache 2',
    packages=['fectl'],
    rust_extensions=[
        RustExtension('fectl.fectl', 'Cargo.toml',
                      binding=Binding.Exec, script=True, strip=Strip.All)],
    install_requires=install_requires,
    tests_require=tests_require,
    include_package_data=True,
    zip_safe=False,
    cmdclass=dict(build_ext=ve_build_ext, test=PyTest),
)

try:
    setup(**setup_args)
except BuildFailed:
    raise
    print("************************************************************")
    print("Cannot compile C accelerator module, use pure python version")
    print("************************************************************")
    setup(**setup_args)
