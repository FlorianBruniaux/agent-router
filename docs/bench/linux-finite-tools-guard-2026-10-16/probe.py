"""Public synthetic canary. No model, credential or private input."""
import ctypes
import errno
import json
import mmap
import os
from pathlib import Path
import resource
import socket
import stat
import sys
import time


def refused(action, allowed=(errno.EPERM, errno.EACCES, errno.EROFS, errno.ENOENT)):
    try:
        result = action()
    except OSError as error:
        assert error.errno in allowed, (error.errno, allowed)
    else:
        if hasattr(result, 'close'):
            result.close()
        raise AssertionError('operation unexpectedly allowed')


def common():
    assert resource.getrlimit(resource.RLIMIT_AS) == (8589934592, 8589934592)
    assert resource.getrlimit(resource.RLIMIT_CORE) == (0, 0)
    assert os.readlink('/proc/self/ns/pid') != os.environ['GUARD_PARENT_PID_NS']
    assert Path('/proc/1/status').is_file()
    # Reading another process's namespace descriptor is deliberately restricted.
    # The actual self PID-namespace identity differs from the outer controller.
    assert resource.getrlimit(resource.RLIMIT_FSIZE) == (16777216, 16777216)
    with mmap.mmap(-1, 16*1024*1024, prot=0):
        pass
    refused(lambda: mmap.mmap(-1, 9*1024*1024*1024, prot=0), (errno.ENOMEM,))
    libc = ctypes.CDLL(None, use_errno=True)
    raised = (ctypes.c_ulonglong * 2)(9*1024*1024*1024, 9*1024*1024*1024)
    assert libc.setrlimit(resource.RLIMIT_AS, ctypes.byref(raised)) == -1
    assert ctypes.get_errno() == errno.EPERM
    assert Path('/candidate/statement.md').read_text() == 'public synthetic input\n'
    for number in range(3,256):
        try: os.fstat(number)
        except OSError as error: assert error.errno == errno.EBADF
        else: raise AssertionError('unexpected inherited descriptor')
    for path in [os.environ['GUARD_PRIVATE_CONTROL'], '/work', '/router-secret-home/config.toml']:
        refused(lambda p=path: open(p, 'rb'), (errno.ENOENT,))
    refused(lambda: open('/candidate/statement.md', 'wb'), (errno.EPERM, errno.EACCES, errno.EROFS))
    refused(lambda: os.chmod('/candidate/statement.md', 0o600), (errno.EPERM, errno.EACCES, errno.EROFS))
    refused(lambda: os.utime('/candidate/statement.md', (1, 1)), (errno.EPERM, errno.EACCES, errno.EROFS))
    libc = ctypes.CDLL(None, use_errno=True)
    assert libc.unshare(0) == -1 and ctypes.get_errno() == errno.EPERM
    status = Path('/proc/self/status').read_text().splitlines()
    caps = [line.split(':')[1].strip() for line in status if line.startswith(('CapInh:', 'CapPrm:', 'CapEff:', 'CapBnd:', 'CapAmb:'))]
    assert len(caps) == 5 and all(int(value, 16) == 0 for value in caps)
    assert any(line == 'NoNewPrivs:\t1' for line in status)
    a, b = socket.socketpair()
    a.sendall(b'x'); assert b.recv(1) == b'x'; a.close(); b.close()


def heartbeat(root, hold):
    path = root/'heartbeat'
    before = path.stat().st_size if path.exists() else 0
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
    child = os.fork()
    if child == 0:
        os.setsid()
        heartbeat_stream = os.dup(1)
        os.close(0); os.close(1); os.close(2)
        for _ in range(3000):
            os.write(fd, b'x')
            os.write(heartbeat_stream, b'~')
            time.sleep(0.01)
        os._exit(0)
    os.close(fd)
    time.sleep(0.3)
    assert path.stat().st_size > before + 1
    print(json.dumps({'probe':'guard-v1','child':child,'memory_bytes':8589934592,'hold':hold,'pid_namespace':os.readlink('/proc/self/ns/pid')}), flush=True)
    if hold:
        while True:
            time.sleep(0.01)


def tools(hold):
    common()
    refused(lambda: socket.socketpair(socket.AF_UNIX,socket.SOCK_SEQPACKET))
    refused(lambda: socket.socket(socket.AF_UNIX,socket.SOCK_SEQPACKET))
    for family, kind in [(socket.AF_INET,socket.SOCK_STREAM), (socket.AF_INET,socket.SOCK_DGRAM), (socket.AF_INET6,socket.SOCK_STREAM), (socket.AF_UNIX,socket.SOCK_STREAM)]:
        refused(lambda f=family,k=kind: socket.socket(f,k))
    assert Path('/tool-guard').is_file()
    refused(lambda: open('/tool-guard','rb'),(errno.EPERM,errno.EACCES))
    refused(lambda: open('/home/config.toml','rb'))
    scratch = Path('/candidate/.scratch')
    assert os.statvfs(scratch).f_blocks * os.statvfs(scratch).f_frsize == 16777216
    public = Path('/candidate/statement.md')
    original = (public.read_bytes(), stat.S_IMODE(public.stat().st_mode), public.stat().st_mtime_ns)
    alias = scratch/'public-alias'; alias.symlink_to(public)
    refused(lambda: os.chmod(alias,0o600),(errno.EPERM,errno.EACCES,errno.EROFS))
    refused(lambda: os.utime(alias,(1,1)),(errno.EPERM,errno.EACCES,errno.EROFS))
    with public.open('rb') as handle:
        via_fd=Path('/proc/self/fd')/str(handle.fileno())
        assert via_fd.read_bytes()==original[0]
        refused(lambda: os.fchmod(handle.fileno(),0o600),(errno.EPERM,errno.EACCES,errno.EROFS))
        refused(lambda: os.utime(handle.fileno(),(1,1)),(errno.EPERM,errno.EACCES,errno.EROFS))
        refused(lambda: os.chmod(via_fd,0o600),(errno.EPERM,errno.EACCES,errno.EROFS))
        refused(lambda: os.utime(via_fd,(1,1)),(errno.EPERM,errno.EACCES,errno.EROFS))
    assert original==(public.read_bytes(),stat.S_IMODE(public.stat().st_mode),public.stat().st_mtime_ns)
    alias.unlink()
    output = scratch/'permissions'
    output.write_bytes(b'scratch-positive')
    output.chmod(0o700); os.utime(output,(1,1))
    assert stat.S_IMODE(output.stat().st_mode) == 0o700 and int(output.stat().st_mtime) == 1
    # No automatic enlargement if the kernel byte cap refuses a write.
    full = scratch/'full'
    with full.open('wb',buffering=0) as handle:
        try:
            for _ in range(17): handle.write(b'x'*(1024*1024))
        except OSError as error:
            assert error.errno in (errno.ENOSPC, errno.EFBIG)
        else: raise AssertionError('scratch byte cap not enforced')
    full.unlink()
    heartbeat(scratch, hold)


def native(role, broker, denied):
    common()
    with socket.create_connection(('127.0.0.1',broker),timeout=2) as stream:
        stream.sendall(b'GET /probe HTTP/1.0\r\n\r\n')
        assert stream.recv(256).startswith(b'HTTP/1.0 200')
    refused(lambda: socket.create_connection(('127.0.0.1',denied),timeout=1))
    for family,kind in [(socket.AF_INET,socket.SOCK_DGRAM),(socket.AF_INET6,socket.SOCK_STREAM)]:
        refused(lambda f=family,k=kind: socket.socket(f,k))
    refused(lambda: socket.socket(socket.AF_UNIX,socket.SOCK_SEQPACKET))
    refused(lambda: socket.socketpair(socket.AF_UNIX,socket.SOCK_DGRAM))
    if role.startswith('legacy'):
        refused(lambda: socket.socketpair(socket.AF_UNIX,socket.SOCK_SEQPACKET))
        refused(lambda: socket.socket(socket.AF_UNIX,socket.SOCK_STREAM))
    else:
        a,b=socket.socketpair(socket.AF_UNIX,socket.SOCK_SEQPACKET|socket.SOCK_CLOEXEC)
        a.send(b'native-spawn-control'); assert b.recv(64)==b'native-spawn-control'; a.close(); b.close()
        refused(lambda: socket.socketpair(socket.AF_INET,socket.SOCK_SEQPACKET))
        assert sorted(p.name for p in Path('/router-tools').iterdir())==['rpc.sock']
        assert stat.S_ISSOCK(Path('/router-tools/rpc.sock').stat().st_mode)
        with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as stream:
            stream.settimeout(2); stream.connect('/router-tools/rpc.sock')
            stream.sendall(b'bounded-local-canary'); assert stream.recv(32)==b'owner-ok'
        refused(lambda: open('/router-tools/unapproved','wb'))
        assert Path('/finite-client').is_file()
    refused(lambda: open('/home/config.toml','wb'),(errno.EPERM,errno.EACCES,errno.EROFS))
    refused(lambda: os.chmod('/home/config.toml',0o600),(errno.EPERM,errno.EACCES,errno.EROFS))
    if role == 'legacy-write':
        target=Path('/candidate/output.rs')
        assert target.read_text()=='before\n'
        target.write_text('scoped-positive\n')
        refused(lambda: os.chmod(target,0o700),(errno.EPERM,errno.EACCES,errno.EROFS))
        refused(lambda: target.unlink(),(errno.EPERM,errno.EACCES,errno.EROFS))
    heartbeat(Path('/home/tmp'), False)


if __name__ == '__main__':
    if sys.argv[1] == 'tools': tools('--hold' in sys.argv)
    else: native(sys.argv[1],int(sys.argv[2]),int(sys.argv[3]))
