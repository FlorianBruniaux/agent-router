#!/usr/bin/env python3
"""Exercise only the fixed MCP bridge. No native SDK, model or provider."""
import array
import fcntl
import json
from pathlib import Path
import socket
import subprocess
import sys
import termios
import threading
import time


def main():
    executable=Path(sys.argv[1]).resolve(strict=True)
    path=Path('/router-tools/rpc.sock')
    assert not path.exists()
    server=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM)
    server.bind(str(path));server.listen(1);server.settimeout(5)
    process=subprocess.Popen([str(executable),'--mcp-tools'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env={'PATH':'/usr/bin:/bin'})
    started=time.monotonic()
    connection,_=server.accept();connection.settimeout(100)
    sent=[]
    def send():
        try:
            for index in range(128):
                frame=json.dumps({'jsonrpc':'2.0','id':index,'result':{'padding':'x'*30000}}).encode()+b'\n'
                connection.sendall(frame);sent.append(index)
        except OSError:
            pass
    sender=threading.Thread(target=send);sender.start()
    try:
        time.sleep(1)
        pending=array.array('i',[0]);fcntl.ioctl(process.stdout.fileno(),termios.FIONREAD,pending,True)
        assert pending[0]>0 and process.poll() is None, 'no observed bridge backpressure'
        # Keep stdin open and never read stdout. Only the fixed bridge's own
        # deadline should terminate this deliberately blocked transport.
        code=process.wait(timeout=104)
        elapsed=time.monotonic()-started
        assert code==125 and 85<=elapsed<=104,(code,elapsed)
        print(json.dumps({'bridge_watchdog':'passed','exit_code':code,'elapsed_seconds':elapsed,'stdout_bytes_pending':pending[0],'complete_frames_sent':len(sent),'models_started':0}))
    finally:
        if process.poll() is None:process.kill();process.wait(timeout=5)
        connection.close();server.close();sender.join(5)
        assert not sender.is_alive()
        for stream in (process.stdin,process.stdout,process.stderr):stream.close()
        path.unlink()


if __name__=='__main__':main()
