"""Linux guard qualification harness. No Runtime proof minting or model calls."""
import concurrent.futures
import fcntl
import hashlib
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import tempfile
import threading
import time
import unittest

BASE=Path(__file__).resolve().parent
HELPER=Path('/work/source/target/release/router-sandbox-helper')
PENDING=b'router-runtime-lease-v1:pending\n'
CLEAN=b'router-runtime-lease-v1:namespace-clean\n'
LIMITS=['--cpu-seconds','300','--address-space-bytes','8589934592','--max-open-files','256','--max-processes','256','--max-file-bytes','16777216']
RUST='/usr/local/rustup/toolchains/1.85.1-aarch64-unknown-linux-gnu/bin'


class GuardTests(unittest.TestCase):
    observations=[]
    @classmethod
    def setUpClass(cls):
        assert Path('/sys/fs/cgroup/memory.max').read_text().strip()=='8589934592'
        assert Path('/sys/fs/cgroup/memory.swap.max').read_text().strip()=='0'
        assert hashlib.sha256(HELPER.read_bytes()).hexdigest()=='d1cc1e8658059c6a5040bdd27d9cc8e37a49fc4923a0243ae070c1bcc96a80d4'
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='guard-case-',dir='/task')
        self.root=Path(self.temp.name)
        self.candidate=self.root/'candidate'; self.candidate.mkdir(mode=0o700)
        (self.candidate/'.scratch').mkdir(mode=0o700)
        (self.candidate/'probe.py').write_bytes((BASE/'probe.py').read_bytes())
        (self.candidate/'statement.md').write_text('public synthetic input\n')
        self.toolchain=self.root/'toolchain-bin'; self.toolchain.mkdir(mode=0o700)
        wrapper=self.toolchain/'rustc'
        wrapper.write_text('#!/usr/bin/python3\nimport os,sys\nreal="'+RUST+'/rustc"\nos.execv(real,[real,"-C","linker=/usr/bin/aarch64-linux-gnu-gcc-12",*sys.argv[1:]])\n')
        wrapper.chmod(0o500)
        self.home=self.root/'home'; self.home.mkdir(mode=0o700)
        (self.home/'tmp').mkdir(mode=0o700)
        (self.home/'config.toml').write_text('protected fixture\n')
        (self.root/'private-control').write_text('unmounted synthetic private control\n')
        self.sockets=self.root/'sockets'; self.sockets.mkdir(mode=0o700)
        self.broker=socket.socket(); self.broker.bind(('0.0.0.0',0)); self.broker.listen(8); self.broker.settimeout(0.1)
        self.other=socket.socket(); self.other.bind(('127.0.0.1',0)); self.other.listen(8)
        self.rpc=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM); self.rpc.bind(str(self.sockets/'rpc.sock')); self.rpc.listen(8); self.rpc.settimeout(0.1)
        self.stop=threading.Event(); self.errors=[]; self.threads=[]
        for kind,server in [('http',self.broker),('unix',self.rpc)]:
            def loop(kind=kind,server=server):
                while not self.stop.is_set():
                    try: conn,_=server.accept()
                    except socket.timeout: continue
                    except OSError: return
                    with conn:
                        conn.settimeout(2)
                        try:
                            data=conn.recv(4096)
                            if kind=='http':
                                assert data.startswith(b'GET /probe HTTP/1.0')
                                conn.sendall(b'HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\nok')
                            else:
                                assert data==b'bounded-local-canary'; conn.sendall(b'owner-ok')
                        except Exception as error: self.errors.append(type(error).__name__)
            thread=threading.Thread(target=loop,daemon=True);thread.start();self.threads.append(thread)
    def tearDown(self):
        self.stop.set();self.broker.close();self.other.close();self.rpc.close()
        for thread in self.threads:thread.join(3)
        self.temp.cleanup()
        self.assertEqual(self.errors,[])
    def command(self,mode,guard,hold=False,feedback=False):
        cmd=['/usr/bin/bwrap','--die-with-parent','--new-session','--unshare-all']
        if mode!='tools': cmd+=['--share-net']
        cmd+=['--cap-drop','ALL','--tmpfs','/','--ro-bind','/usr','/usr','--symlink','usr/bin','/bin','--symlink','usr/lib','/lib','--symlink','usr/lib64','/lib64','--proc','/proc','--dev','/dev','--ro-bind',str(self.candidate),'/candidate']
        if mode=='tools':
            cmd+=['--ro-bind',str(self.toolchain),'/usr/local/bin']
            cmd+=['--size','16777216','--tmpfs','/candidate/.scratch','--symlink','/candidate/.scratch','/tmp','--ro-bind',str(guard),'/tool-guard','--remount-ro','/','--chdir','/candidate','--','/tool-guard','--tool-exec-v1','/candidate/.scratch','--','/usr/bin/python3']
            cmd+=['-B','/candidate/feedback.py'] if feedback else ['/candidate/probe.py','tools']+(['--hold'] if hold else [])
        else:
            cmd+=['--ro-bind',str(self.home),'/home','--bind',str(self.home/'tmp'),'/home/tmp','--ro-bind',str(self.sockets),'/router-tools','--ro-bind','/usr/bin/true','/finite-client']
            writes=[]
            if mode=='legacy-write':
                output=self.candidate/'output.rs';output.write_text('before\n')
                cmd+=['--bind',str(output),'/candidate/output.rs'];writes=['/candidate/output.rs']
            for i in range(5):
                source=guard.parent if i==0 else self.root/f'asset-{i}'
                if i:source.mkdir(exist_ok=True)
                cmd+=['--ro-bind',str(source),f'/assets/a{i}']
            cmd+=['--remount-ro','/','--chdir','/candidate','--',f'/assets/a0/{guard.name}','--restrict-v1' if mode.startswith('legacy') else '--restrict-tools-v1',str(self.broker.getsockname()[1]),'/home','/candidate',str(len(writes)),*writes,'5',*[f'/assets/a{i}' for i in range(5)],'--','/usr/bin/python3','/candidate/probe.py',mode,str(self.broker.getsockname()[1]),str(self.other.getsockname()[1])]
        return cmd
    def run_guard(self,mode,*,baseline=False,hold=False,eof=False,feedback=False,bad_grant=False,bad_scratch=False):
        guard=BASE/('guard-before' if baseline else 'guard-after')
        command=self.command(mode,guard,hold=hold,feedback=feedback)
        if bad_grant:
            index=command.index('--restrict-tools-v1')
            command[index+4]='1';command.insert(index+5,'/candidate/statement.md')
        if bad_scratch:command[command.index('--tool-exec-v1')+1]='/candidate'
        lease=self.root/f'lease-{len(self.observations)}';lease.write_bytes(PENDING);lease.chmod(0o600)
        env={'PATH':'/usr/local/bin:'+RUST+':/usr/bin:/bin','HOME':'/unavailable','TMPDIR':'/candidate/.scratch','ROUTER_RUNTIME_SUPERVISION':'framed-v1','ROUTER_RUNTIME_CONTAINMENT':'pid-namespace-v1','ROUTER_RUNTIME_LEASE':str(lease),'GUARD_PARENT_PID_NS':os.readlink('/proc/self/ns/pid'),'GUARD_PRIVATE_CONTROL':str(self.root/'private-control')}
        timeout=800 if hold and not eof else 20000
        process=subprocess.Popen([str(HELPER),*LIMITS,'--supervise-v1',*command],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,bufsize=0)
        process.stdin.write(b'ARLIFE01'+struct.pack('>QQQ',0,timeout,100));process.stdin.flush()
        pool=concurrent.futures.ThreadPoolExecutor(2)
        positive=threading.Event()
        def read_stream(stream, observe=False):
            result=bytearray()
            while True:
                chunk=stream.read(4096)
                if not chunk:return bytes(result)
                result.extend(chunk)
                assert len(result)<=65536,'bounded stream'
                if observe and b'guard-v1' in result:positive.set()
        out=pool.submit(read_stream,process.stdout,True)
        err=pool.submit(read_stream,process.stderr)
        try:
            if eof:
                assert positive.wait(10),'positive canary must run before parent EOF'
                process.stdin.close()
            code=process.wait(timeout=30)
        finally:
            if not process.stdin.closed:process.stdin.close()
            if process.poll() is None:process.wait(timeout=5)
        stdout=out.result(3);stderr=err.result(3);pool.shutdown(wait=True)
        process.stdout.close();process.stderr.close()
        self.assertLessEqual(len(stdout)+len(stderr),65536)
        self.assertEqual(lease.read_bytes(),CLEAN)
        if not feedback and not baseline and not bad_grant and not bad_scratch:
            self.assertGreater(stdout.count(b'~'),1,'detached writer was observed before stream EOF: '+stderr.decode())
        with lease.open('r+b') as handle:fcntl.flock(handle,fcntl.LOCK_EX|fcntl.LOCK_NB)
        record={'mode':mode,'baseline':baseline,'hold':hold,'parent_eof':eof,'feedback':feedback,'exit_code':code,'namespace_clean':True,'stdout':stdout.decode(),'stderr':stderr.decode()}
        self.observations.append(record)
        if mode!='tools' and not baseline and not bad_grant:
            path=self.home/'tmp/heartbeat';self.assertGreater(path.stat().st_size,1)
            lengths=[]
            for _ in range(3):lengths.append(path.stat().st_size);time.sleep(0.02)
            self.assertEqual(len(set(lengths)),1)
        return code,stdout,stderr
    def test_legacy_profile_still_enforces_the_original_boundary(self):
        for old in (True,False):
            code,out,err=self.run_guard('legacy',baseline=old)
            self.assertEqual((code,err),(0,b''),out.decode()+err.decode())
    def test_legacy_scoped_write_retains_the_existing_one_file_boundary(self):
        for old in (True,False):
            code,out,err=self.run_guard('legacy-write',baseline=old)
            self.assertEqual((code,err),(0,b''),out.decode()+err.decode())
            self.assertEqual((self.candidate/'output.rs').read_text(),'scoped-positive\n')
            self.assertEqual((self.candidate/'statement.md').read_text(),'public synthetic input\n')
    def test_native_tools_profile_allows_the_mounted_local_ipc_endpoint(self):
        code,out,err=self.run_guard('native-tools')
        self.assertEqual((code,err),(0,b''),out.decode()+err.decode())
    def test_tool_profile_has_no_network_and_only_scratch_writes(self):
        code,out,err=self.run_guard('tools')
        self.assertEqual((code,err),(0,b''),out.decode()+err.decode())
    def test_unpatched_guard_refuses_the_new_profile_before_the_child(self):
        code,out,err=self.run_guard('tools',baseline=True)
        self.assertEqual(code,125);self.assertEqual(out,b'');self.assertIn(b'linux-isolation:arguments',err)
    def test_native_profile_refuses_an_explicit_workspace_write_grant(self):
        code,out,err=self.run_guard('native-tools',bad_grant=True)
        self.assertEqual(code,125);self.assertEqual(out,b'');self.assertIn(b'tool-native-no-write',err)
        self.assertEqual((self.candidate/'statement.md').read_text(),'public synthetic input\n')
    def test_tool_profile_refuses_a_different_scratch_root(self):
        code,out,err=self.run_guard('tools',bad_scratch=True)
        self.assertEqual(code,125);self.assertEqual(out,b'');self.assertIn(b'tool-scratch',err)
    def test_timeout_stops_the_detached_tool_namespace(self):
        code,out,err=self.run_guard('tools',hold=True)
        self.assertEqual((code,err),(124,b''),out.decode()+err.decode());self.assertIn(b'guard-v1',out)
    def test_parent_eof_stops_the_detached_tool_namespace(self):
        code,out,err=self.run_guard('tools',hold=True,eof=True)
        self.assertEqual((code,err),(125,b''),out.decode()+err.decode());self.assertIn(b'guard-v1',out)
    def test_actual_supplied_feedback_runs_red_then_green_within_the_scratch_cap(self):
        (self.candidate/'feedback.py').write_bytes((BASE/'feedback.py').read_bytes())
        (self.candidate/'feedback-tests.rs').write_text('#[path="lib.rs"] mod code; #[test] fn useful_check(){ assert_eq!(code::answer(),42); }\n')
        (self.candidate/'lib.rs').write_text('pub fn answer()->u32{41}\n')
        code,out,err=self.run_guard('tools',feedback=True)
        self.assertEqual((code,err),(1,b''),out.decode()+err.decode());self.assertIn(b'test result: FAILED',out)
        # This controller effect is authored and occurs only after the exact
        # previous namespace lease is clean. No tool/model supplies this write.
        (self.candidate/'lib.rs').write_text('pub fn answer()->u32{42}\n')
        code,out,err=self.run_guard('tools',feedback=True)
        self.assertEqual((code,err),(0,b''),out.decode()+err.decode());self.assertIn(b'1 passed',out)
        self.assertEqual((self.candidate/'statement.md').read_text(),'public synthetic input\n')


if __name__=='__main__':
    result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(GuardTests))
    (BASE/'results.json').write_text(json.dumps({'tests':result.testsRun,'failures':len(result.failures),'errors':len(result.errors),'models_started':0,'memory_max':8589934592,'swap_max':0,'observations':GuardTests.observations},indent=2)+'\n')
    raise SystemExit(not result.wasSuccessful())
