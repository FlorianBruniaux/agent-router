/* PRIVATE INERT CANDIDATE. Requires parent protocol approval before execution. */
#include <libproc.h>
#include <mach/mach.h>
#include <sys/resource.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <errno.h>
#include <inttypes.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#define MIB UINT64_C(1048576)
#define SAMPLE_CAP 256
static volatile sig_atomic_t interrupted;
struct owned_child { pid_t pid; int retained, reaped, status, signals, blocked_after_reap; };
struct observation { uint64_t ns, bytes, start; };
static struct observation observations[SAMPLE_CAP];
static unsigned count;
static uint64_t monotonic_ns(void) {
    struct timespec t;
    if (clock_gettime(CLOCK_MONOTONIC,&t)) return 0;
    return (uint64_t)t.tv_sec*UINT64_C(1000000000)+(uint64_t)t.tv_nsec;
}
static void on_stop(int signo) { (void)signo; interrupted=1; }
static void pause_poll(void) { struct timespec t={0,10000000}; (void)nanosleep(&t,NULL); }
static void record_reap(struct owned_child *c,int status) { c->retained=0;c->reaped=1;c->status=status; }
static int reap_now(struct owned_child *c) {
    if (!c->retained) return c->reaped;
    int status=0;
    pid_t got=waitpid(c->pid,&status,WNOHANG);
    if (got==c->pid) { record_reap(c,status);return 1; }
    if (got<0 && errno!=EINTR) return -1;
    return 0;
}
/* Only lifecycle code sees a signal target: the owned, unreaped fork result. */
static int stop_owned(struct owned_child *c) {
    if (!c->retained || c->pid<=0) { c->blocked_after_reap++;return 0; }
    int reaped=reap_now(c);
    if (reaped) return reaped>0?0:-1;
    c->signals++;
    return kill(c->pid,SIGKILL);
}
static int reap_until(struct owned_child *c,uint64_t end) {
    do { int r=reap_now(c);if (r) return r>0;pause_poll(); } while(monotonic_ns()<end);
    return 0;
}
static int query(pid_t owned_pid,struct rusage_info_v0 *out,int *saved_errno) {
    memset(out,0,sizeof(*out));errno=0;
    int rc=proc_pid_rusage(owned_pid,RUSAGE_INFO_V0,(rusage_info_t *)out);
    *saved_errno=rc?errno:0;
    return rc;
}
static int append_sample(struct rusage_info_v0 *s) {
    if (count>=SAMPLE_CAP) return -1;
    observations[count++]=(struct observation){monotonic_ns(),s->ri_phys_footprint,s->ri_proc_start_abstime};
    return 0;
}
static int send_byte(int fd,char value) { return write(fd,&value,1)==1?0:-1; }
static void child_main(int commands,int events,size_t bytes) {
    signal(SIGALRM,SIG_DFL);signal(SIGTERM,SIG_DFL);alarm(4);
    struct rlimit core={0};
    if (getrlimit(RLIMIT_CORE,&core) || core.rlim_cur || core.rlim_max) _exit(90);
    if (send_byte(events,'R')) _exit(91);
    char command;
    if (read(commands,&command,1)!=1) _exit(0);
    if (command=='E') _exit(0);
    if (command!='A') _exit(92);
    if (bytes) {
        void *memory=mmap(NULL,bytes,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);
        if (memory==MAP_FAILED) _exit(93);
        long page=sysconf(_SC_PAGESIZE);
        if (page<=0) _exit(94);
        volatile unsigned char *p=memory;
        for(size_t n=0;n<bytes;n+=(size_t)page) p[n]=(unsigned char)(1+(n/(size_t)page)%251);
        p[bytes-1]=173;
        /* Keep mapping alive until exit or signal, no second allocation. */
    }
    if(send_byte(events,'D')) _exit(95);
    while(read(commands,&command,1)==1) if(command=='E') _exit(0);
    _exit(0);
}
/* SELF control: own ephemeral process only, exactly one 16 MiB setter call. */
static int self_limit_case(void) {
    signal(SIGALRM,SIG_DFL);alarm(4);
    struct rlimit core={0,0},core_after={0},as_before={0},as_after={0};
    if(setrlimit(RLIMIT_CORE,&core)||getrlimit(RLIMIT_CORE,&core_after)||core_after.rlim_cur||core_after.rlim_max||getrlimit(RLIMIT_AS,&as_before)) return 77;
    task_vm_info_data_t vm={0};mach_msg_type_number_t count=TASK_VM_INFO_COUNT;
    kern_return_t info_rc=task_info(mach_task_self(),TASK_VM_INFO,(task_info_t)&vm,&count);
    if(info_rc!=KERN_SUCCESS||count<TASK_VM_INFO_REV1_COUNT||vm.phys_footprint>=8*MIB) {
        printf("{\"case\":\"self\",\"classification\":\"UnknownBaseline\",\"task_info_kr\":%d,\"vm_count\":%u,\"baseline_bytes\":%" PRIu64 ",\"setter_called\":false,\"qualification\":false}\n",info_rc,count,(uint64_t)vm.phys_footprint);
        return 77;
    }
    int old_limit=0;
    kern_return_t kr=task_set_phys_footprint_limit(mach_task_self(),16,&old_limit);
    printf("{\"case\":\"self\",\"event\":\"setter\",\"classification\":\"%s\",\"mach_return\":%d,\"requested_mib\":16,\"old_limit_valid\":%s,\"old_limit_mib\":%d,\"baseline_bytes\":%" PRIu64 ",\"allocation_budget_bytes\":%" PRIu64 ",\"qualification\":false}\n",kr==KERN_SUCCESS?"InstalledOnly":kr==KERN_NO_ACCESS?"Refused":"FailureUnknown",kr,kr==KERN_SUCCESS?"true":"false",old_limit,(uint64_t)vm.phys_footprint,kr==KERN_SUCCESS?32*MIB:UINT64_C(0));
    fflush(stdout);
    if(kr!=KERN_SUCCESS) return kr==KERN_NO_ACCESS?0:77;
    size_t bytes=32*MIB;
    void *memory=mmap(NULL,bytes,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);
    if(memory==MAP_FAILED) {puts("{\"case\":\"self\",\"event\":\"allocation_failed\",\"qualification\":false}");return 77;}
    long page=sysconf(_SC_PAGESIZE);if(page<=0) return 77;
    volatile unsigned char *data=memory;
    for(size_t offset=0;offset<bytes;offset+=(size_t)page) data[offset]=(unsigned char)(1+(offset/(size_t)page)%251);
    data[bytes-1]=173;
    memset(&vm,0,sizeof(vm));
    count=TASK_VM_INFO_COUNT;info_rc=task_info(mach_task_self(),TASK_VM_INFO,(task_info_t)&vm,&count);
    int sample_valid=info_rc==KERN_SUCCESS&&count>=TASK_VM_INFO_REV1_COUNT;
    char footprint[32]="null";
    if(sample_valid) snprintf(footprint,sizeof(footprint),"%" PRIu64,(uint64_t)vm.phys_footprint);
    int as_ok=!getrlimit(RLIMIT_AS,&as_after)&&as_before.rlim_cur==as_after.rlim_cur&&as_before.rlim_max==as_after.rlim_max;
    printf("{\"case\":\"self\",\"event\":\"touch_complete\",\"touched_allocation_bytes\":%zu,\"task_info_kr\":%d,\"vm_count\":%u,\"phys_footprint_bytes\":%s,\"phys_footprint_valid\":%s,\"as_unchanged\":%s,\"classification\":\"InstalledOnly\",\"qualification\":false}\n",bytes,info_rc,count,footprint,sample_valid?"true":"false",as_ok?"true":"false");
    return as_ok&&sample_valid?0:77;
}
int main(int argc,char **argv) {
    static const char *modes[]={"normal","growth","failread","mismatch","deadline","self"};
    if(argc!=2) return 64;
    int mode=-1;
    for(unsigned n=0;n<sizeof(modes)/sizeof(modes[0]);n++) if(!strcmp(argv[1],modes[n])) mode=(int)n;
    if(mode<0) return 64;
    if(mode==5) return self_limit_case();
    struct sigaction child_action={0};child_action.sa_handler=SIG_DFL;sigemptyset(&child_action.sa_mask);
    if(sigaction(SIGCHLD,&child_action,NULL)) return 77; /* prohibit inherited auto-reap */
    struct sigaction sa={0};sa.sa_handler=on_stop;sigemptyset(&sa.sa_mask);
    if(sigaction(SIGALRM,&sa,NULL)||sigaction(SIGTERM,&sa,NULL)) return 77;
    signal(SIGPIPE,SIG_IGN);alarm(3);
    uint64_t began=monotonic_ns();if(!began) return 77;
    struct rlimit core={0,0},core_effective={0},as_before={0},as_after={0};
    if(getrlimit(RLIMIT_AS,&as_before)||setrlimit(RLIMIT_CORE,&core)||getrlimit(RLIMIT_CORE,&core_effective)||core_effective.rlim_cur||core_effective.rlim_max) return 77;
    int commands[2],events[2];
    if(pipe(commands)) return 77;
    if(pipe(events)) {close(commands[0]);close(commands[1]);return 77;}
    size_t bytes=mode==0?4*MIB:mode==1?24*MIB:0;
    pid_t pid=fork();
    if(pid<0) {close(commands[0]);close(commands[1]);close(events[0]);close(events[1]);return 77;}
    if(!pid) {close(commands[1]);close(events[0]);child_main(commands[0],events[1],bytes);}
    close(commands[0]);close(events[1]);
    struct owned_child child={.pid=pid,.retained=1};
    const char *decision="unknown",*reason="startup_unknown";
    int injected=0,query_errno=0,pass=0,baseline_valid=0,touched_confirmed=0;
    uint64_t baseline=0,threshold=0,peak=0,stop_ns=0;
    struct rusage_info_v0 initial={0},sample={0};
    char ack=0;
    if(read(events[0],&ack,1)!=1||ack!='R'||interrupted) goto cleanup;
    if(query(child.pid,&initial,&query_errno)||!initial.ri_proc_start_abstime||initial.ri_proc_exit_abstime) {reason="baseline_query_unknown";goto cleanup;}
    baseline_valid=1;baseline=initial.ri_phys_footprint;peak=baseline;
    if(baseline>UINT64_MAX-8*MIB) {reason="threshold_overflow";goto cleanup;}
    threshold=baseline+8*MIB;
    if(append_sample(&initial)) {reason="sample_cap";goto cleanup;}
    if(send_byte(commands[1],'A')||read(events[0],&ack,1)!=1||ack!='D'||interrupted) {reason="touch_ack_unknown";goto cleanup;}
    touched_confirmed=1;
    uint64_t end=monotonic_ns()+(mode==4?UINT64_C(100000000):UINT64_C(350000000));
    while(!interrupted&&monotonic_ns()<end) {
        if(query(child.pid,&sample,&query_errno)) {reason="query_unknown";goto cleanup;}
        if(mode==2) {injected=1;query_errno=EIO;reason="injected_query_unknown";goto cleanup;}
        if(mode==3) {injected=1;sample.ri_proc_start_abstime^=UINT64_C(1);}
        if(sample.ri_proc_start_abstime!=initial.ri_proc_start_abstime||sample.ri_proc_exit_abstime) {reason="identity_unknown";goto cleanup;}
        if(append_sample(&sample)) {reason="sample_cap";goto cleanup;}
        if(sample.ri_phys_footprint>peak) peak=sample.ri_phys_footprint;
        if(sample.ri_phys_footprint>=threshold) {decision="stop_request";reason="threshold_crossed";stop_ns=monotonic_ns();goto cleanup;}
        if(mode==0&&sample.ri_phys_footprint>=baseline+MIB) {
            decision="completed";reason="measured_under_threshold";
            close(commands[1]);commands[1]=-1;
            pass=reap_until(&child,monotonic_ns()+UINT64_C(300000000))&&WIFEXITED(child.status)&&WEXITSTATUS(child.status)==0&&child.signals==0;
            if(!pass) {decision="unknown";reason="normal_exit_unconfirmed";}
            goto finish;
        }
        pause_poll();
    }
    if(mode==4&&!interrupted) {decision="stop_request";reason="elapsed_deadline";stop_ns=monotonic_ns();}
    else reason=interrupted?"observer_interrupted":"expected_observation_missing";
cleanup:
    if(child.retained) (void)stop_owned(&child);
    if(!reap_until(&child,monotonic_ns()+UINT64_C(500000000))) {decision="unknown";reason="cleanup_unconfirmed";pass=0;}
    else if(baseline_valid&&child.signals==1&&WIFSIGNALED(child.status)&&WTERMSIG(child.status)==SIGKILL) {
        pass=(mode==1&&!strcmp(reason,"threshold_crossed"))||(mode==2&&injected&&!strcmp(reason,"injected_query_unknown"))||(mode==3&&injected&&!strcmp(reason,"identity_unknown"))||(mode==4&&!strcmp(reason,"elapsed_deadline"));
    }
finish:
    if(commands[1]>=0) close(commands[1]);
    close(events[0]);
    if(child.retained) { (void)stop_owned(&child);(void)reap_until(&child,monotonic_ns()+UINT64_C(500000000));pass=0; }
    int as_ok=!getrlimit(RLIMIT_AS,&as_after)&&as_after.rlim_cur==as_before.rlim_cur&&as_after.rlim_max==as_before.rlim_max;
    if(!as_ok||!child.reaped) pass=0;
    if(!child.reaped) {decision="unknown";reason="cleanup_unconfirmed";}
    printf("{\"case\":\"%s\",\"fixture_pass\":%s,\"qualification\":false,\"decision\":\"%s\",\"reason\":\"%s\",\"pid\":%d,\"start_abstime\":%" PRIu64 ",\"baseline_valid\":%s,\"baseline_bytes\":%" PRIu64 ",\"threshold_bytes\":%" PRIu64 ",\"peak_observed_bytes\":%" PRIu64 ",\"allocation_budget_bytes\":%zu,\"touch_ack\":%s,\"injected\":%s,\"query_errno\":%d,\"monitor_signal_attempts\":0,\"owned_target_only\":true,\"lifecycle_signal_attempts\":%d,\"blocked_after_reap\":%d,\"reaped\":%s,\"wait_status\":%d,\"as_unchanged\":%s,\"core_zero\":true,\"stop_ns\":%" PRIu64 ",\"duration_ns\":%" PRIu64 ",\"samples\":[",modes[mode],pass?"true":"false",decision,reason,child.pid,initial.ri_proc_start_abstime,baseline_valid?"true":"false",baseline,threshold,peak,bytes,touched_confirmed?"true":"false",injected?"true":"false",query_errno,child.signals,child.blocked_after_reap,child.reaped?"true":"false",child.status,as_ok?"true":"false",stop_ns,monotonic_ns()-began);
    for(unsigned i=0;i<count;i++) printf("%s{\"ns\":%" PRIu64 ",\"bytes\":%" PRIu64 "}",i?",":"",observations[i].ns,observations[i].bytes);
    puts("]}");
    return pass?0:77;
}
