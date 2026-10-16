/* Explicit Linux diagnostic, no product launch authority. Linux ABI 6 required. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <linux/audit.h>
#include <linux/filter.h>
#include <linux/seccomp.h>
#include <signal.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/prctl.h>
#include <sys/resource.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <unistd.h>
#include <arpa/inet.h>

#define BOUND UINT64_C(8589934592)
#define CREATE 444
#define ADD 445
#define RESTRICT 446
#define FS_EXEC (1ULL << 0)
#define FS_WRITE (1ULL << 1)
#define FS_READ (1ULL << 2)
#define FS_READDIR (1ULL << 3)
#define FS_REMOVE_DIR (1ULL << 4)
#define FS_REMOVE_FILE (1ULL << 5)
#define FS_MAKE_DIR (1ULL << 7)
#define FS_MAKE_REG (1ULL << 8)
#define FS_REFER (1ULL << 13)
#define FS_TRUNCATE (1ULL << 14)
#define READ (FS_EXEC | FS_READ | FS_READDIR)
#define ALL_FS ((1ULL << 15) - 1)
struct ruleset { uint64_t fs, net, scoped; };
struct path_rule { uint64_t access; int32_t fd; } __attribute__((packed));
struct net_rule { uint64_t access, port; };

static void fail(const char *s) { fprintf(stderr,"linux-isolation:%s:%d\n",s,errno); exit(125); }
static void check(int ok,const char *s) { if(!ok) fail(s); }
static unsigned port(const char *s) {
    char *end; errno=0; unsigned long p=strtoul(s,&end,10);
    check(!errno && *s && !*end && p>0 && p<65536,"port"); return (unsigned)p;
}
static void path(int rules,const char *name,uint64_t rights) {
    int fd=open(name,O_PATH|O_CLOEXEC|O_NOFOLLOW); if(fd<0) fail("path");
    struct stat st; if(fstat(fd,&st)) fail("stat");
    if(S_ISREG(st.st_mode)) rights &= FS_EXEC|FS_READ|FS_WRITE|FS_TRUNCATE;
    struct path_rule rule={rights,fd};
    if(syscall(ADD,rules,1,&rule,0)) fail("path-rule");
    close(fd);
}
#define DENY(n) BPF_JUMP(BPF_JMP|BPF_JEQ|BPF_K,(n),0,1), BPF_STMT(BPF_RET|BPF_K,SECCOMP_RET_ERRNO|EPERM)
static void seccomp(void) {
    /* TCP/IPv4 stream creation only. No DNS/UDP, IPv6, Unix or raw sockets. */
    struct sock_filter code[]={
      BPF_STMT(BPF_LD|BPF_W|BPF_ABS,offsetof(struct seccomp_data,arch)),
      BPF_JUMP(BPF_JMP|BPF_JEQ|BPF_K,AUDIT_ARCH_AARCH64,1,0),
      BPF_STMT(BPF_RET|BPF_K,SECCOMP_RET_KILL_PROCESS),
      BPF_STMT(BPF_LD|BPF_W|BPF_ABS,offsetof(struct seccomp_data,nr)),
      DENY(SYS_ptrace), DENY(SYS_process_vm_readv), DENY(SYS_process_vm_writev),
      DENY(SYS_pidfd_getfd), DENY(SYS_mount), DENY(SYS_umount2), DENY(SYS_setns),
      DENY(SYS_unshare), DENY(SYS_bpf), DENY(SYS_perf_event_open),
      DENY(SYS_fchmod), DENY(SYS_fchmodat), DENY(452), /* fchmodat2 */
      DENY(SYS_fchown), DENY(SYS_fchownat), DENY(SYS_utimensat),
      DENY(SYS_io_uring_setup),
      DENY(SYS_setxattr), DENY(SYS_fsetxattr), DENY(SYS_lsetxattr),
      DENY(SYS_removexattr), DENY(SYS_fremovexattr), DENY(SYS_lremovexattr),
      /* Tokio's signal self-pipe uses anonymous Unix stream pairs, no addresses. */
      BPF_JUMP(BPF_JMP|BPF_JEQ|BPF_K,SYS_socketpair,0,9),
      BPF_STMT(BPF_LD|BPF_W|BPF_ABS,offsetof(struct seccomp_data,args[0])),
      BPF_JUMP(BPF_JMP|BPF_JEQ|BPF_K,AF_UNIX,0,5),
      BPF_STMT(BPF_LD|BPF_W|BPF_ABS,offsetof(struct seccomp_data,args[1])),
      BPF_STMT(BPF_ALU|BPF_AND|BPF_K,~(SOCK_NONBLOCK|SOCK_CLOEXEC)),
      BPF_JUMP(BPF_JMP|BPF_JEQ|BPF_K,SOCK_STREAM,0,2),
      BPF_STMT(BPF_LD|BPF_W|BPF_ABS,offsetof(struct seccomp_data,args[2])),
      BPF_JUMP(BPF_JMP|BPF_JEQ|BPF_K,0,1,0),
      BPF_STMT(BPF_RET|BPF_K,SECCOMP_RET_ERRNO|EPERM),
      BPF_STMT(BPF_RET|BPF_K,SECCOMP_RET_ALLOW),
      BPF_STMT(BPF_LD|BPF_W|BPF_ABS,offsetof(struct seccomp_data,nr)),
      BPF_JUMP(BPF_JMP|BPF_JEQ|BPF_K,SYS_socket,0,9),
      BPF_STMT(BPF_LD|BPF_W|BPF_ABS,offsetof(struct seccomp_data,args[0])),
      BPF_JUMP(BPF_JMP|BPF_JEQ|BPF_K,AF_INET,0,6),
      BPF_STMT(BPF_LD|BPF_W|BPF_ABS,offsetof(struct seccomp_data,args[1])),
      BPF_STMT(BPF_ALU|BPF_AND|BPF_K,~(SOCK_NONBLOCK|SOCK_CLOEXEC)),
      BPF_JUMP(BPF_JMP|BPF_JEQ|BPF_K,SOCK_STREAM,0,3),
      BPF_STMT(BPF_LD|BPF_W|BPF_ABS,offsetof(struct seccomp_data,args[2])),
      BPF_JUMP(BPF_JMP|BPF_JEQ|BPF_K,0,2,0),
      BPF_JUMP(BPF_JMP|BPF_JEQ|BPF_K,IPPROTO_TCP,1,0),
      BPF_STMT(BPF_RET|BPF_K,SECCOMP_RET_ERRNO|EPERM),
      BPF_STMT(BPF_RET|BPF_K,SECCOMP_RET_ALLOW)
    };
    struct sock_fprog program={(unsigned short)(sizeof(code)/sizeof(code[0])),code};
    if(prctl(PR_SET_SECCOMP,SECCOMP_MODE_FILTER,&program)) fail("seccomp");
}
static void denied(int result,const char *name) {
    check(result<0 && (errno==EACCES || errno==EPERM),name);
}
static void denied_file(int result,const char *name) {
    check(result<0 && (errno==EACCES || errno==EPERM || errno==EROFS),name);
}
static int connect_port(unsigned p) {
    int fd=socket(AF_INET,SOCK_STREAM|SOCK_CLOEXEC,0); if(fd<0) fail("tcp-socket");
    struct timeval timeout={1,0}; setsockopt(fd,SOL_SOCKET,SO_RCVTIMEO,&timeout,sizeof(timeout));
    struct sockaddr_in a={.sin_family=AF_INET,.sin_port=htons((uint16_t)p),.sin_addr={htonl(INADDR_LOOPBACK)}};
    int result=connect(fd,(struct sockaddr*)&a,sizeof(a)); int saved=errno;
    if(!result) { char c; check(read(fd,&c,1)==1 && c=='F',"broker-banner"); }
    close(fd); errno=saved; return result;
}
static void canary(int argc,char **argv) {
    /* six protected paths, broker, other port, protected store, allowed file, heartbeat */
    check(argc==13 || argc==14,"canary-args");
    struct rlimit as, core; check(!getrlimit(RLIMIT_AS,&as) && as.rlim_cur==BOUND && as.rlim_max==BOUND,"as-bound");
    check(!getrlimit(RLIMIT_CORE,&core) && core.rlim_cur==0 && core.rlim_max==0,"core-bound");
    void *small=mmap(NULL,16*1024*1024,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
    check(small!=MAP_FAILED,"small-map"); munmap(small,16*1024*1024);
    errno=0; void *large=mmap(NULL,(size_t)9*1024*1024*1024,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
    check(large==MAP_FAILED && errno==ENOMEM,"large-map-refusal");
    check(!connect_port(port(argv[8])),"broker-allow");
    denied(connect_port(port(argv[9])),"other-port-denial");
    denied(socket(AF_INET,SOCK_DGRAM,0),"udp-denial");
    denied(socket(AF_INET6,SOCK_STREAM,0),"ipv6-denial");
    denied(socket(AF_UNIX,SOCK_STREAM,0),"unix-denial");
    int fd=socket(AF_INET,SOCK_STREAM,0); check(fd>=0,"bind-socket");
    struct sockaddr_in a={.sin_family=AF_INET,.sin_port=0,.sin_addr={htonl(INADDR_LOOPBACK)}};
    denied(bind(fd,(struct sockaddr*)&a,sizeof(a)),"bind-denial"); close(fd);
    for(int asset=0;asset<(argc==14 ? 7 : 6);asset++) {
        int i=asset<6 ? asset+2 : 13;
        denied_file(open(argv[i],O_WRONLY|O_TRUNC),"asset-write-denial");
        denied_file(truncate(argv[i],0),"asset-truncate-denial");
        denied_file(unlink(argv[i]),"asset-unlink-denial");
        char target[4096]; check(snprintf(target,sizeof(target),"%s.moved",argv[i])<(int)sizeof(target),"path-size");
        denied_file(rename(argv[i],target),"asset-rename-denial");
        denied_file(chmod(argv[i],0700),"asset-chmod-denial");
        char parent[4096]; check(strlen(argv[i])<sizeof(parent),"parent-size"); strcpy(parent,argv[i]);
        char *slash=strrchr(parent,'/'); check(slash!=NULL,"parent-path"); *slash=0;
        check(snprintf(target,sizeof(target),"%s/extra",parent)<(int)sizeof(target),"create-size");
        denied_file(open(target,O_WRONLY|O_CREAT|O_EXCL,0600),"asset-directory-create-denial");
        check(snprintf(target,sizeof(target),"%s.moved",parent)<(int)sizeof(target),"rename-size");
        denied_file(rename(parent,target),"asset-directory-rename-denial");
    }
    denied(open(argv[10],O_RDONLY),"store-read-denial");
    fd=open(argv[11],O_WRONLY|O_TRUNC); check(fd>=0,"scope-write");
    check(write(fd,"scope-ok",8)==8,"scope-content"); close(fd);
    /* The detached writer is a trusted fixed canary, not an arbitrary process. */
    int ready[2]; check(!pipe2(ready,O_CLOEXEC),"ready-pipe");
    pid_t child=fork(); check(child>=0,"fork");
    if(!child) {
        close(ready[0]); check(setsid()>0,"setsid"); close(0); close(1); close(2);
        fd=open(argv[12],O_WRONLY|O_APPEND|O_CLOEXEC); check(fd>=0,"heartbeat-open");
        check(write(ready[1],"R",1)==1,"ready-write"); close(ready[1]);
        for(unsigned i=0;i<10000;i++) { if(write(fd,"x",1)!=1) _exit(2); usleep(10000); }
        _exit(0);
    }
    close(ready[1]); char ready_byte; check(read(ready[0],&ready_byte,1)==1 && ready_byte=='R',"ready-read"); close(ready[0]);
    printf("{\"memory\":\"8GiB\",\"broker\":\"exact-port\",\"protected_assets\":%d,\"detached_pid\":%ld}\n",argc==14 ? 7 : 6,(long)child);
}
int main(int argc,char **argv) {
    if(argc>1 && !strcmp(argv[1],"--canary")) { canary(argc,argv); return 0; }
    /* port, six immutable parent directories, home/tmp, exact writable file, heartbeat, program, args */
    check(argc>=13 && !strcmp(argv[1],"--restrict"),"argv"); check(syscall(CREATE,NULL,0,1)>=6,"abi6-required");
    struct ruleset rules={ALL_FS,3,3};
    int fd=(int)syscall(CREATE,&rules,sizeof(rules),0); if(fd<0) fail("create-ruleset");
    for(int i=3;i<9;i++) path(fd,argv[i],READ);
    path(fd,"/usr",READ); path(fd,"/etc",READ); path(fd,"/proc",FS_READ|FS_READDIR);
    char workspace[4096]; check(strlen(argv[10])<sizeof(workspace),"scope-path-size"); strcpy(workspace,argv[10]); char *slash=strrchr(workspace,'/'); check(slash!=NULL,"scope-path"); *slash=0; path(fd,workspace,READ);
    path(fd,argv[9],ALL_FS); path(fd,argv[10],FS_READ|FS_WRITE|FS_TRUNCATE); path(fd,argv[11],FS_WRITE);
    path(fd,"/dev/null",FS_READ|FS_WRITE); path(fd,"/dev/urandom",FS_READ);
    struct net_rule net={2,port(argv[2])}; if(syscall(ADD,fd,2,&net,0)) fail("net-rule");
    if(prctl(PR_SET_NO_NEW_PRIVS,1,0,0,0) || syscall(RESTRICT,fd,0)) fail("restrict");
    close(fd); seccomp(); execv(argv[12],&argv[12]); fail("exec");
}
