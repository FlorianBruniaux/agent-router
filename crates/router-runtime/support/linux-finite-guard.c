/* Fixed Linux finite-client guard. Applied only by the reviewed backend. ABI 6 required. */
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
#include <net/if.h>
#include <sys/ioctl.h>

#if defined(__aarch64__)
#define GUARD_ARCH AUDIT_ARCH_AARCH64
#elif defined(__x86_64__)
#define GUARD_ARCH AUDIT_ARCH_X86_64
#else
#error unsupported seccomp architecture
#endif

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
/* Landlock CONNECT_TCP binds a port, not an address. The shared namespace
 * must have only its actual loopback device and no routes/capabilities. The
 * controller separately retains an exclusive wildcard reservation of the port.
 * Thus another address cannot host another service on the permitted port. */
static void isolated_network(void) {
    FILE *f=fopen("/proc/thread-self/net/dev","re");check(f!=NULL,"network-devices");
    int interface_fd=socket(AF_INET,SOCK_STREAM|SOCK_CLOEXEC,0);check(interface_fd>=0,"interface-socket");
    char line[8192];check(fgets(line,sizeof(line),f)!=NULL,"devices-header");
    check(fgets(line,sizeof(line),f)!=NULL,"devices-columns");unsigned rows=0;
    unsigned loopbacks=0;
    while(fgets(line,sizeof(line),f)) {
        char name[32],extra;unsigned long long n[16];
        int got=sscanf(line," %31[^:]: %llu %llu %llu %llu %llu %llu %llu %llu %llu %llu %llu %llu %llu %llu %llu %llu %c",name,&n[0],&n[1],&n[2],&n[3],&n[4],&n[5],&n[6],&n[7],&n[8],&n[9],&n[10],&n[11],&n[12],&n[13],&n[14],&n[15],&extra);
        check(got==17&&strlen(name)<IFNAMSIZ&&++rows<=64,"device-bound");
        struct ifreq request={0};strcpy(request.ifr_name,name);
        check(!ioctl(interface_fd,SIOCGIFFLAGS,&request),"device-flags");
        if(!strcmp(name,"lo")) {
            check((request.ifr_flags&(IFF_UP|IFF_LOOPBACK))==(IFF_UP|IFF_LOOPBACK)&&++loopbacks==1,"actual-loopback");
        } else check(!(request.ifr_flags&(IFF_UP|IFF_RUNNING)),"no-active-interface");
    }
    check(!ferror(f)&&loopbacks==1,"devices-bound");fclose(f);close(interface_fd);
    f=fopen("/proc/thread-self/net/route","re");check(f!=NULL,"network-routes");
    check(fgets(line,sizeof(line),f)!=NULL,"routes-header");
    check(fgets(line,sizeof(line),f)==NULL&&!ferror(f),"no-routes");fclose(f);
    f=fopen("/proc/thread-self/status","re");check(f!=NULL,"network-capabilities");
    unsigned masks=0,nnp=0;
    while(fgets(line,sizeof(line),f)) {
        const char *keys[]={"CapInh:","CapPrm:","CapEff:","CapBnd:","CapAmb:"};
        for(unsigned i=0;i<5;i++)if(!strncmp(line,keys[i],strlen(keys[i]))) {
            char *end;errno=0;unsigned long long value=strtoull(line+strlen(keys[i]),&end,16);
            check(!errno&&value==0&&(*end=='\n'||*end==0)&&!(masks&(1u<<i)),"no-network-capability");masks|=1u<<i;
        }
        if(!strncmp(line,"NoNewPrivs:",11)) {check(!nnp&&strtoul(line+11,NULL,10)==1,"no-new-privileges");nnp++;}
    }
    check(!ferror(f)&&masks==31&&nnp==1,"capability-bound");fclose(f);
}
#define DENY(n) BPF_JUMP(BPF_JMP|BPF_JEQ|BPF_K,(n),0,1), BPF_STMT(BPF_RET|BPF_K,SECCOMP_RET_ERRNO|EPERM)
static void seccomp(void) {
    /* TCP/IPv4 stream creation only. No DNS/UDP, IPv6, Unix or raw sockets. */
    struct sock_filter code[]={
      BPF_STMT(BPF_LD|BPF_W|BPF_ABS,offsetof(struct seccomp_data,arch)),
      BPF_JUMP(BPF_JMP|BPF_JEQ|BPF_K,GUARD_ARCH,1,0),
      BPF_STMT(BPF_RET|BPF_K,SECCOMP_RET_KILL_PROCESS),
      BPF_STMT(BPF_LD|BPF_W|BPF_ABS,offsetof(struct seccomp_data,nr)),
      DENY(SYS_ptrace), DENY(SYS_process_vm_readv), DENY(SYS_process_vm_writev),
      DENY(SYS_pidfd_getfd), DENY(SYS_mount), DENY(SYS_umount2), DENY(SYS_setns),
      DENY(SYS_unshare), DENY(SYS_bpf), DENY(SYS_perf_event_open),
      #ifdef SYS_chmod
      DENY(SYS_chmod),
#endif
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
    if(!result) { const char *q="GET /__router_probe__ HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n";
        check(write(fd,q,strlen(q))==(ssize_t)strlen(q),"broker-probe-write");
        char c; check(read(fd,&c,1)==1 && c=='H',"broker-probe-response"); }
    close(fd); errno=saved; return result;
}

static unsigned count(const char *s) {
    char *end;errno=0;unsigned long n=strtoul(s,&end,10);
    check(!errno && *s && !*end && n<=64,"count");return (unsigned)n;
}
static void probe(unsigned broker,unsigned denied_port,const char *hidden,const char *home,const char *workspace,const char *control,const char *nonce,unsigned nr,char **reads,unsigned nw,char **writes) {
    check(strlen(nonce)==64,"nonce");
    for(unsigned i=0;i<64;i++) check((nonce[i]>='0'&&nonce[i]<='9')||(nonce[i]>='a'&&nonce[i]<='f'),"nonce-hex");
    struct rlimit as,core;check(!getrlimit(RLIMIT_AS,&as)&&as.rlim_cur==BOUND&&as.rlim_max==BOUND,"as-bound");
    check(!getrlimit(RLIMIT_CORE,&core)&&!core.rlim_cur&&!core.rlim_max,"core-bound");
    struct rlimit cpu,files,pids,size;
    check(!getrlimit(RLIMIT_CPU,&cpu)&&cpu.rlim_cur==300&&cpu.rlim_max==300,"cpu-bound");
    check(!getrlimit(RLIMIT_NOFILE,&files)&&files.rlim_cur==256&&files.rlim_max==256,"descriptor-bound");
    check(!getrlimit(RLIMIT_NPROC,&pids)&&pids.rlim_cur==256&&pids.rlim_max==256,"process-bound");
    check(!getrlimit(RLIMIT_FSIZE,&size)&&size.rlim_cur==16777216&&size.rlim_max==16777216,"file-bound");
    void *small=mmap(NULL,16*1024*1024,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
    check(small!=MAP_FAILED,"small-map");munmap(small,16*1024*1024);
    errno=0;void *large=mmap(NULL,(size_t)9*1024*1024*1024,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
    check(large==MAP_FAILED&&errno==ENOMEM,"large-map-refusal");
    check(!connect_port(broker),"broker-allow");denied(connect_port(denied_port),"other-port-denial");
    int external=socket(AF_INET,SOCK_STREAM|SOCK_CLOEXEC,0);check(external>=0,"external-socket");
    struct sockaddr_in remote={.sin_family=AF_INET,.sin_port=htons((uint16_t)broker)};
    check(inet_pton(AF_INET,"192.0.2.1",&remote.sin_addr)==1,"external-address");
    check(connect(external,(struct sockaddr*)&remote,sizeof(remote))<0&&errno==ENETUNREACH,"external-route-refusal");close(external);
    denied(socket(AF_INET,SOCK_DGRAM,0),"udp-denial");denied(socket(AF_INET6,SOCK_STREAM,0),"ipv6-denial");denied(socket(AF_UNIX,SOCK_STREAM,0),"unix-denial");
    int fd=socket(AF_INET,SOCK_STREAM,0);check(fd>=0,"bind-socket");
    struct sockaddr_in address={.sin_family=AF_INET,.sin_port=0,.sin_addr={htonl(INADDR_LOOPBACK)}};
    denied(bind(fd,(struct sockaddr*)&address,sizeof(address)),"bind-denial");close(fd);
    fd=open(hidden,O_RDONLY|O_CLOEXEC);check(fd<0&&(errno==ENOENT||errno==EACCES||errno==EPERM),"private-read-denial");
    char pathbuf[4096];
    const char *configs[]={"config.toml","local-model-catalog.json"};
    for(unsigned i=0;i<2;i++) {
        check(snprintf(pathbuf,sizeof(pathbuf),"%s/%s",home,configs[i])<(int)sizeof(pathbuf),"configuration-path");
        denied_file(open(pathbuf,O_WRONLY|O_TRUNC),"configuration-write-denial");
        denied_file(chmod(pathbuf,0600),"configuration-chmod-denial");
        denied_file(unlink(pathbuf),"configuration-unlink-denial");
    }
    check(snprintf(pathbuf,sizeof(pathbuf),"%s/.router-denied-%s",workspace,nonce)<(int)sizeof(pathbuf),"outside-path");
    denied_file(open(pathbuf,O_WRONLY|O_CREAT|O_EXCL,0600),"scope-denial");
    if(nw) {
        check(nw==1,"one-approved-file");
        fd=open(writes[0],O_WRONLY|O_TRUNC|O_CLOEXEC);check(fd>=0,"scoped-positive-open");
        check(write(fd,nonce,64)==64&&!fsync(fd),"scoped-positive-write");close(fd);
    }
    for(unsigned i=0;i<nr;i++) {
        check(snprintf(pathbuf,sizeof(pathbuf),"%s/program",reads[i])<(int)sizeof(pathbuf),"asset-path");
        denied_file(open(pathbuf,O_WRONLY|O_TRUNC),"asset-write-denial");
        denied_file(chmod(pathbuf,0700),"asset-chmod-denial");
        denied_file(unlink(pathbuf),"asset-unlink-denial");
    }
    check(snprintf(pathbuf,sizeof(pathbuf),"%s/control",control)<(int)sizeof(pathbuf),"control-path");
    fd=open(pathbuf,O_WRONLY|O_CREAT|O_EXCL|O_CLOEXEC,0600);check(fd>=0,"positive-file");
    check(write(fd,nonce,64)==64,"positive-write");close(fd);
    check(snprintf(pathbuf,sizeof(pathbuf),"%s/heartbeat",control)<(int)sizeof(pathbuf),"heartbeat-path");
    fd=open(pathbuf,O_WRONLY|O_CREAT|O_EXCL|O_CLOEXEC,0600);check(fd>=0,"heartbeat-create");
    pid_t child=fork();check(child>=0,"detached-fork");
    if(!child) { check(setsid()>0,"detached-session");close(0);close(1);close(2);
        for(unsigned i=0;i<3000;i++){ if(write(fd,"x",1)!=1)_exit(2);usleep(10000); }_exit(0); }
    close(fd);usleep(350000);
    struct stat heartbeat;check(!stat(pathbuf,&heartbeat)&&heartbeat.st_size>1,"detached-positive");
    printf("{\"linux_finite_probe\":\"v2\",\"memory_bytes\":8589934592,\"nonce\":\"%s\",\"detached_pid\":%ld,\"workspace_write\":%s}\n",nonce,(long)child,nw?"true":"false");fflush(stdout);
}
int main(int argc,char **argv) {
    check(argc>=10&&!strcmp(argv[1],"--restrict-v1"),"arguments");
    unsigned broker=port(argv[2]);const char *home=argv[3],*workspace=argv[4];
    unsigned nw=count(argv[5]);int cursor=6;check(cursor+(int)nw<argc,"write-arguments");char **writes=&argv[cursor];cursor+=(int)nw;
    unsigned nr=count(argv[cursor++]);check(nr>=5&&cursor+(int)nr<argc,"read-arguments");char **reads=&argv[cursor];cursor+=(int)nr;
    int do_probe=cursor<argc&&!strcmp(argv[cursor],"--probe-v1");unsigned other=0;char *hidden=NULL,*control=NULL,*nonce=NULL;
    if(do_probe) { check(cursor+5<argc,"probe-arguments");other=port(argv[++cursor]);hidden=argv[++cursor];control=argv[++cursor];nonce=argv[++cursor];cursor++; }
    check(cursor+1<argc&&!strcmp(argv[cursor],"--"),"executable");cursor++;
    isolated_network();
    check(syscall(CREATE,NULL,0,1)>=6,"landlock-abi6");struct ruleset rules={ALL_FS,3,3};
    int fd=(int)syscall(CREATE,&rules,sizeof(rules),0);check(fd>=0,"ruleset");
    for(unsigned i=0;i<nr;i++)path(fd,reads[i],READ);
    path(fd,"/usr",READ);path(fd,"/proc",FS_READ|FS_READDIR);
    path(fd,home,READ);path(fd,workspace,READ);
    char tmp[4096];check(snprintf(tmp,sizeof(tmp),"%s/tmp",home)<(int)sizeof(tmp),"tmp-path");path(fd,tmp,ALL_FS);
    for(unsigned i=0;i<nw;i++)path(fd,writes[i],ALL_FS);
    path(fd,"/dev/null",FS_READ|FS_WRITE);path(fd,"/dev/urandom",FS_READ);
    struct net_rule net={2,broker};check(!syscall(ADD,fd,2,&net,0),"network-rule");
    check(!prctl(PR_SET_NO_NEW_PRIVS,1,0,0,0)&&!syscall(RESTRICT,fd,0),"restrict");close(fd);seccomp();
    if(do_probe)probe(broker,other,hidden,home,workspace,control,nonce,nr,reads,nw,writes);
    execv(argv[cursor],&argv[cursor]);fail("exec");
}
