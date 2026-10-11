/* Pure guard regression: no child, signal, allocation or external PID. */
#define main unused_fixture_main
#include "memory_observer_fixture.c"
#undef main
#include <assert.h>
int main(void) {
    struct owned_child c={.pid=0,.retained=1};
    record_reap(&c,0);
    assert(!c.retained && c.reaped);
    assert(stop_owned(&c)==0);
    assert(c.signals==0 && c.blocked_after_reap==1);
    return 0;
}
