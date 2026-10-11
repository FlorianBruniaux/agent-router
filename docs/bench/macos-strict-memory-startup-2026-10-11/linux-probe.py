"""Inert Linux fixture: unchanged hard address-space bound, no network or files written."""
import errno,json,mmap,os,resource,subprocess,sys
BOUND=8589934592
resource.setrlimit(resource.RLIMIT_AS,(BOUND,BOUND))
assert resource.getrlimit(resource.RLIMIT_AS)==(BOUND,BOUND)
assert resource.getrlimit(resource.RLIMIT_CORE)[0]==0
row={"exact_address_space_bytes":BOUND,"installed_soft_hard":list(resource.getrlimit(resource.RLIMIT_AS))}
with mmap.mmap(-1,16*1024*1024,flags=mmap.MAP_PRIVATE|mmap.MAP_ANONYMOUS,prot=0):
 row["small_virtual_mapping_allowed"]=True
try:
 mapping=mmap.mmap(-1,9*1024*1024*1024,flags=mmap.MAP_PRIVATE|mmap.MAP_ANONYMOUS,prot=0)
except OSError as e:
 assert e.errno==errno.ENOMEM
 row["oversize_mapping_refused_errno"]=e.errno
else:
 mapping.close()
 raise AssertionError("oversize virtual mapping unexpectedly accepted")
for hard in (BOUND+1024*1024,resource.RLIM_INFINITY):
 try: resource.setrlimit(resource.RLIMIT_AS,(hard,hard))
 except (ValueError,PermissionError): pass
 else: raise AssertionError("hard bound raised or removed")
row["raising_and_removing_hard_bound_refused"]=True
read_fd,write_fd=os.pipe()
pid=os.fork()
if pid==0:
 os.close(read_fd)
 os.write(write_fd,json.dumps(list(resource.getrlimit(resource.RLIMIT_AS))).encode())
 os.close(write_fd);os._exit(0)
os.close(write_fd);fork_limits=json.loads(os.read(read_fd,1024));os.close(read_fd)
assert os.waitpid(pid,0)==(pid,0)
assert fork_limits==[BOUND,BOUND]
row["fork_inherited_soft_hard"]=fork_limits
child_code="import json,resource;print(json.dumps(list(resource.getrlimit(resource.RLIMIT_AS))))"
r=subprocess.run([sys.executable,"-I","-c",child_code],stdin=subprocess.DEVNULL,capture_output=True,text=True,timeout=5,env={"PATH":"/usr/bin:/bin"})
assert r.returncode==0 and not r.stderr
exec_limits=json.loads(r.stdout)
assert exec_limits==[BOUND,BOUND]
row["exec_inherited_soft_hard"]=exec_limits
for filename in ("memory.max","memory.swap.max","pids.max"):
 with open("/sys/fs/cgroup/"+filename) as f: row[filename]=f.read().strip()
assert row["memory.max"]==str(BOUND) and row["memory.swap.max"]=="0" and row["pids.max"]=="16"
row.update({"uid":os.getuid(),"kernel":os.uname().release,"architecture":os.uname().machine,"diagnostic_only":True,"native_qualification":False})
assert row["uid"]==65534
print(json.dumps(row))
