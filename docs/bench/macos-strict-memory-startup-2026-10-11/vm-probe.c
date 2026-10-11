/* Inert local diagnostic fixture. No allocator, mmap, exec or provider call. */
#include <mach/mach.h>
#include <mach/mach_vm.h>
#include <mach/vm_region.h>
#include <sys/resource.h>
#include <errno.h>
#include <stdint.h>
#include <inttypes.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#define BOUND UINT64_C(8589934592)
/* Geometry only, from XNU f6217f891ac0bb64f3d375211650a4c1ff8ca1ea,
 * osfmk/mach/arm/vm_param.h:159-162. This does not identify the installed kernel. */
#define PINNED_GPU_START UINT64_C(0x1000000000)
#define PINNED_GPU_SIZE UINT64_C(0x6000000000)
#define CAP 4096
#define DEPTH_CAP 16
struct snapshot {
    mach_task_basic_info_data_t basic;
    task_vm_info_data_t vm;
    mach_msg_type_number_t vm_count;
    kern_return_t basic_rc, vm_rc;
};
struct row { uint64_t address, size; uint32_t depth, tag; int prot, max_prot, submap, share; };
struct walk {
    struct row rows[CAP];
    unsigned count, calls;
    uint64_t total, no_access;
    int complete, stop; /* 0 end, 1 cap, 2 depth, 3 overflow/order, 4 kernel */
    kern_return_t last_rc;
};
static struct snapshot entry, before, after;
static struct walk top, leaf;
static void snapshot(struct snapshot *s) {
    mach_msg_type_number_t n = MACH_TASK_BASIC_INFO_COUNT;
    s->basic_rc = task_info(mach_task_self(), MACH_TASK_BASIC_INFO, (task_info_t)&s->basic, &n);
    s->vm_count = TASK_VM_INFO_COUNT;
    s->vm_rc = task_info(mach_task_self(), TASK_VM_INFO, (task_info_t)&s->vm, &s->vm_count);
}
static void walk_regions(struct walk *w, int recurse) {
    mach_vm_address_t address = 0;
    natural_t depth = 0;
    while (w->calls < CAP) {
        mach_vm_address_t requested = address;
        mach_vm_size_t size = 0;
        vm_region_submap_info_data_64_t info = {0};
        mach_msg_type_number_t count = VM_REGION_SUBMAP_INFO_COUNT_64;
        if (!recurse) depth = 0;
        w->calls++;
        w->last_rc = mach_vm_region_recurse(mach_task_self(), &address, &size, &depth,
                                           (vm_region_recurse_info_t)&info, &count);
        if (w->last_rc == KERN_INVALID_ADDRESS) { w->complete = 1; return; }
        if (w->last_rc != KERN_SUCCESS) { w->stop = 4; return; }
        if (!size || address < requested || size > UINT64_MAX - address) { w->stop = 3; return; }
        if (recurse && info.is_submap) {
            if (depth >= DEPTH_CAP) { w->stop = 2; return; }
            depth++;
            continue;
        }
        if (size > UINT64_MAX - w->total) { w->stop = 3; return; }
        struct row *r = &w->rows[w->count++];
        *r = (struct row){address, size, depth, info.user_tag, info.protection, info.max_protection,
                          info.is_submap, info.share_mode};
        w->total += size;
        if (info.protection == VM_PROT_NONE) w->no_access += size;
        address += size;
    }
    w->stop = 1;
}
static void print_snapshot(const char *phase, struct snapshot *s) {
    printf("{\"event\":\"task_info\",\"phase\":\"%s\",\"basic_rc\":%d,\"basic_virtual_bytes\":%" PRIu64 ",\"resident_bytes\":%" PRIu64 ",\"vm_rc\":%d,\"vm_count\":%u,\"vm_virtual_bytes\":%" PRIu64 ",\"region_count\":%d,\"phys_footprint_bytes\":%" PRIu64 "}\n",
           phase,s->basic_rc,(uint64_t)s->basic.virtual_size,(uint64_t)s->basic.resident_size,
           s->vm_rc,s->vm_count,(uint64_t)s->vm.virtual_size,s->vm.region_count,(uint64_t)s->vm.phys_footprint);
}
static void print_walk(const char *name, struct walk *w) {
    printf("{\"event\":\"region_summary\",\"walk\":\"%s\",\"complete\":%s,\"stop\":%d,\"last_rc\":%d,\"calls\":%u,\"rows\":%u,\"total_bytes\":%" PRIu64 ",\"no_access_bytes\":%" PRIu64 "}\n",name,w->complete?"true":"false",w->stop,w->last_rc,w->calls,w->count,w->total,w->no_access);
    for (unsigned i=0;i<w->count;i++) {
        struct row *r=&w->rows[i];
        printf("{\"event\":\"region\",\"walk\":\"%s\",\"index\":%u,\"size_bytes\":%" PRIu64 ",\"depth\":%u,\"user_tag\":%u,\"protection\":%d,\"max_protection\":%d,\"is_submap\":%s,\"share_mode\":%d,\"matches_pinned_gpu_carveout\":%s}\n",name,i,r->size,r->depth,r->tag,r->prot,r->max_prot,r->submap?"true":"false",r->share, (r->address == PINNED_GPU_START && r->size == PINNED_GPU_SIZE) ? "true" : "false");
    }
}
int main(int argc,char **argv) {
    if (argc!=2 || strcmp(argv[1],"--exact-8gib")) return 64;
    /* No formatted output or explicit allocation until all observations end. */
    snapshot(&entry);
    struct rlimit inherited={0}, effective={0};
    int get_before=getrlimit(RLIMIT_AS,&inherited);
    int get_before_errno=get_before<0?errno:0;
    walk_regions(&top,0);
    walk_regions(&leaf,1);
    snapshot(&before);
    struct rlimit requested={BOUND,BOUND};
    errno=0;
    int set_rc=setrlimit(RLIMIT_AS,&requested);
    int set_errno=set_rc<0?errno:0;
    int get_after=getrlimit(RLIMIT_AS,&effective);
    int get_after_errno=get_after<0?errno:0;
    snapshot(&after);
    printf("{\"event\":\"identity\",\"pid\":%d,\"diagnostic_only\":true,\"qualification\":false,\"bound_bytes\":%" PRIu64 ",\"rlim_infinity\":%" PRIu64 "}\n",getpid(),BOUND,(uint64_t)RLIM_INFINITY);
    print_snapshot("entry",&entry);
    print_walk("top",&top);
    print_walk("leaf",&leaf);
    print_snapshot("before_setrlimit",&before);
    printf("{\"event\":\"setrlimit\",\"get_before_rc\":%d,\"get_before_errno\":%d,\"before_soft\":%" PRIu64 ",\"before_hard\":%" PRIu64 ",\"requested_soft\":%" PRIu64 ",\"requested_hard\":%" PRIu64 ",\"rc\":%d,\"errno\":%d,\"get_after_rc\":%d,\"get_after_errno\":%d,\"effective_soft\":%" PRIu64 ",\"effective_hard\":%" PRIu64 "}\n",get_before,get_before_errno,(uint64_t)inherited.rlim_cur,(uint64_t)inherited.rlim_max,BOUND,BOUND,set_rc,set_errno,get_after,get_after_errno,(uint64_t)effective.rlim_cur,(uint64_t)effective.rlim_max);
    print_snapshot("after_setrlimit",&after);
    return set_rc<0?125:0;
}
