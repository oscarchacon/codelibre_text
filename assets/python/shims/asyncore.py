"""``asyncore`` (removed in Python 3.12), enough for the plugins that still use it: a socket
dispatcher driven by ``select``. Same names and behaviour as the standard library's."""

import os
import select
import socket
import sys
import time
from errno import (EAGAIN, EALREADY, EBADF, ECONNABORTED, ECONNRESET, EINPROGRESS, EINVAL, EISCONN, ENOTCONN,
                   EPIPE, ESHUTDOWN, EWOULDBLOCK, errorcode)

_DISCONNECTED = frozenset({ECONNRESET, ENOTCONN, ESHUTDOWN, ECONNABORTED, EPIPE, EBADF})

try:
    socket_map
except NameError:
    socket_map = {}


class ExitNow(Exception):
    pass


_reraised_exceptions = (ExitNow, KeyboardInterrupt, SystemExit)


def _strerror(err):
    try:
        return os.strerror(err)
    except (ValueError, OverflowError, NameError):
        return errorcode.get(err, "Unknown error %s" % err)


def read(obj):
    try:
        obj.handle_read_event()
    except _reraised_exceptions:
        raise
    except Exception:
        obj.handle_error()


def write(obj):
    try:
        obj.handle_write_event()
    except _reraised_exceptions:
        raise
    except Exception:
        obj.handle_error()


def _exception(obj):
    try:
        obj.handle_expt_event()
    except _reraised_exceptions:
        raise
    except Exception:
        obj.handle_error()


def readwrite(obj, flags):
    try:
        if flags & select.POLLIN:
            obj.handle_read_event()
        if flags & select.POLLOUT:
            obj.handle_write_event()
        if flags & select.POLLPRI:
            obj.handle_expt_event()
        if flags & (select.POLLHUP | select.POLLERR | select.POLLNVAL):
            obj.handle_close()
    except OSError as e:
        if e.errno not in _DISCONNECTED:
            obj.handle_error()
        else:
            obj.handle_close()
    except _reraised_exceptions:
        raise
    except Exception:
        obj.handle_error()


def poll(timeout=0.0, map=None):
    if map is None:
        map = socket_map
    if not map:
        return
    r, w, e = [], [], []
    for fd, obj in list(map.items()):
        is_r = obj.readable()
        is_w = obj.writable()
        if is_r:
            r.append(fd)
        if is_w and not obj.accepting:
            w.append(fd)
        if is_r or is_w:
            e.append(fd)
    if [] == r == w == e:
        time.sleep(timeout)
        return
    r, w, e = select.select(r, w, e, timeout)
    for fd in r:
        obj = map.get(fd)
        if obj is not None:
            read(obj)
    for fd in w:
        obj = map.get(fd)
        if obj is not None:
            write(obj)
    for fd in e:
        obj = map.get(fd)
        if obj is not None:
            _exception(obj)


poll2 = poll3 = poll


def loop(timeout=30.0, use_poll=False, map=None, count=None):
    if map is None:
        map = socket_map
    if count is None:
        while map:
            poll(timeout, map)
    else:
        while map and count > 0:
            poll(timeout, map)
            count -= 1


class dispatcher:
    debug = False
    connected = False
    accepting = False
    connecting = False
    closing = False
    addr = None
    ignore_log_types = frozenset({"warning"})

    def __init__(self, sock=None, map=None):
        self._map = socket_map if map is None else map
        self._fileno = None
        if sock:
            sock.setblocking(False)
            self.set_socket(sock, map)
            self.connected = True
            try:
                self.addr = sock.getpeername()
            except OSError as err:
                if err.errno in (ENOTCONN, EINVAL):
                    self.connected = False
                else:
                    self.del_channel(map)
                    raise
        else:
            self.socket = None

    def __repr__(self):
        status = [self.__class__.__module__ + "." + self.__class__.__qualname__]
        if self.accepting and self.addr:
            status.append("listening")
        elif self.connected:
            status.append("connected")
        if self.addr is not None:
            status.append("%s:%s" % self.addr if isinstance(self.addr, tuple) else repr(self.addr))
        return "<%s at %#x>" % (" ".join(status), id(self))

    def add_channel(self, map=None):
        if map is None:
            map = self._map
        map[self._fileno] = self

    def del_channel(self, map=None):
        fd = self._fileno
        if map is None:
            map = self._map
        if fd in map:
            del map[fd]
        self._fileno = None

    def create_socket(self, family=socket.AF_INET, type=socket.SOCK_STREAM):
        self.family_and_type = family, type
        sock = socket.socket(family, type)
        sock.setblocking(False)
        self.set_socket(sock)

    def set_socket(self, sock, map=None):
        self.socket = sock
        self._fileno = sock.fileno()
        self.add_channel(map)

    def set_reuse_addr(self):
        try:
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR,
                                   self.socket.getsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR) | 1)
        except OSError:
            pass

    def readable(self):
        return True

    def writable(self):
        return True

    def listen(self, num):
        self.accepting = True
        if os.name == "nt" and num > 5:
            num = 5
        return self.socket.listen(num)

    def bind(self, addr):
        self.addr = addr
        return self.socket.bind(addr)

    def connect(self, address):
        self.connected = False
        self.connecting = True
        err = self.socket.connect_ex(address)
        if err in (EINPROGRESS, EALREADY, EWOULDBLOCK) or err == EINVAL and os.name == "nt":
            self.addr = address
            return
        if err in (0, EISCONN):
            self.addr = address
            self.handle_connect_event()
        else:
            raise OSError(err, errorcode.get(err, str(err)))

    def accept(self):
        try:
            conn, addr = self.socket.accept()
        except TypeError:
            return None
        except OSError as why:
            if why.errno in (EWOULDBLOCK, ECONNABORTED, EAGAIN):
                return None
            raise
        return conn, addr

    def send(self, data):
        try:
            return self.socket.send(data)
        except OSError as why:
            if why.errno == EWOULDBLOCK:
                return 0
            if why.errno in _DISCONNECTED:
                self.handle_close()
                return 0
            raise

    def recv(self, buffer_size):
        try:
            data = self.socket.recv(buffer_size)
            if not data:
                self.handle_close()
                return b""
            return data
        except OSError as why:
            if why.errno in _DISCONNECTED:
                self.handle_close()
                return b""
            raise

    def close(self):
        self.connected = False
        self.accepting = False
        self.connecting = False
        self.del_channel()
        if self.socket is not None:
            try:
                self.socket.close()
            except OSError as why:
                if why.errno not in (ENOTCONN, EBADF):
                    raise

    def log(self, message):
        sys.stderr.write("log: %s\n" % str(message))

    def log_info(self, message, type="info"):
        if type not in self.ignore_log_types:
            print("%s: %s" % (type, message))

    def handle_read_event(self):
        if self.accepting:
            self.handle_accept()
        elif not self.connected:
            if self.connecting:
                self.handle_connect_event()
            self.handle_read()
        else:
            self.handle_read()

    def handle_connect_event(self):
        err = self.socket.getsockopt(socket.SOL_SOCKET, socket.SO_ERROR)
        if err != 0:
            raise OSError(err, _strerror(err))
        self.handle_connect()
        self.connected = True
        self.connecting = False

    def handle_write_event(self):
        if self.accepting:
            return
        if not self.connected:
            if self.connecting:
                self.handle_connect_event()
        self.handle_write()

    def handle_expt_event(self):
        err = self.socket.getsockopt(socket.SOL_SOCKET, socket.SO_ERROR)
        if err != 0:
            self.handle_close()
        else:
            self.handle_expt()

    def handle_error(self):
        import traceback
        self.log_info("uncaptured python exception, closing channel %s\n%s" % (repr(self), traceback.format_exc()),
                      "error")
        self.handle_close()

    def handle_expt(self):
        self.log_info("unhandled incoming priority event", "warning")

    def handle_read(self):
        self.log_info("unhandled read event", "warning")

    def handle_write(self):
        self.log_info("unhandled write event", "warning")

    def handle_connect(self):
        self.log_info("unhandled connect event", "warning")

    def handle_accept(self):
        pair = self.accept()
        if pair is not None:
            self.handle_accepted(*pair)

    def handle_accepted(self, sock, addr):
        sock.close()
        self.log_info("unhandled accepted event", "warning")

    def handle_close(self):
        self.log_info("unhandled close event", "warning")
        self.close()


class dispatcher_with_send(dispatcher):
    def __init__(self, sock=None, map=None):
        dispatcher.__init__(self, sock, map)
        self.out_buffer = b""

    def initiate_send(self):
        num_sent = dispatcher.send(self, self.out_buffer[:65536])
        self.out_buffer = self.out_buffer[num_sent:]

    handle_write = initiate_send

    def writable(self):
        return (not self.connected) or len(self.out_buffer)

    def send(self, data):
        self.out_buffer = self.out_buffer + data
        self.initiate_send()


def compact_traceback():
    t, v, tb = sys.exc_info()
    tbinfo = []
    if not tb:
        raise AssertionError("traceback does not exist")
    while tb:
        tbinfo.append((tb.tb_frame.f_code.co_filename, tb.tb_frame.f_code.co_name, str(tb.tb_lineno)))
        tb = tb.tb_next
    del tb
    file, function, line = tbinfo[-1]
    info = " ".join("[%s|%s|%s]" % x for x in tbinfo)
    return (file, function, line), t, v, info


def close_all(map=None, ignore_all=False):
    if map is None:
        map = socket_map
    for x in list(map.values()):
        try:
            x.close()
        except OSError as x_error:
            if x_error.errno != EBADF and not ignore_all:
                raise
        except _reraised_exceptions:
            raise
        except Exception:
            if not ignore_all:
                raise
    map.clear()


if os.name == "posix":
    class file_wrapper:
        def __init__(self, fd):
            self.fd = os.dup(fd)

        def recv(self, *args):
            return os.read(self.fd, *args)

        def send(self, *args):
            return os.write(self.fd, *args)

        def getsockopt(self, level, optname, buflen=None):
            if level == socket.SOL_SOCKET and optname == socket.SO_ERROR and not buflen:
                return 0
            raise NotImplementedError("Only asyncore specific behaviour implemented.")

        read = recv
        write = send

        def close(self):
            if self.fd >= 0:
                fd, self.fd = self.fd, -1
                os.close(fd)

        def fileno(self):
            return self.fd

    class file_dispatcher(dispatcher):
        def __init__(self, fd, map=None):
            dispatcher.__init__(self, None, map)
            self.connected = True
            try:
                fd = fd.fileno()
            except AttributeError:
                pass
            self.set_file(fd)
            os.set_blocking(fd, False)

        def set_file(self, fd):
            self.socket = file_wrapper(fd)
            self._fileno = self.socket.fileno()
            self.add_channel()
