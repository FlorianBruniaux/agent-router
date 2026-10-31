// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
/* Linux qualification fixture only, never linked into a product binary.
 * Arm externally after the native worker has terminated. Stop only the exact
 * owner's target FD, before write, after a short write, or after fsync. No constructor.
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <limits.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static atomic_int stopped;

static int matches(int fd, const char *stage) {
    int matched = 0;
    int saved = errno;
    const char *target = getenv("AR_CUT_TARGET");
    const char *arm = getenv("AR_CUT_ARM");
    const char *wanted = getenv("AR_CUT_STAGE");
    char path[64], observed[PATH_MAX], executable[PATH_MAX];
    if (!target || !arm || !wanted || strcmp(wanted, stage) || access(arm, F_OK))
        goto done;
    ssize_t n = readlink("/proc/self/exe", executable, sizeof(executable) - 1);
    if (n < 0) goto done;
    executable[n] = 0;
    if (strcmp(executable, "/task/programs/agent-router")) goto done;
    if (snprintf(path, sizeof(path), "/proc/self/fd/%d", fd) >= (int)sizeof(path))
        goto done;
    n = readlink(path, observed, sizeof(observed) - 1);
    if (n < 0) goto done;
    observed[n] = 0;
    matched = !strcmp(observed, target);
done:
    errno = saved;
    return matched;
}

static void checkpoint(int fd, const char *stage) {
    if (matches(fd, stage) && !atomic_exchange(&stopped, 1)) {
        if (kill(getpid(), SIGSTOP)) _exit(121);
        /* A resumed interrupted fixture must never perform another effect. */
        _exit(122);
    }
}

ssize_t write(int fd, const void *data, size_t size) {
    ssize_t (*real_write)(int, const void *, size_t) = dlsym(RTLD_NEXT, "write");
    if (!real_write) _exit(123);
    if (size > 1 && matches(fd, "partial")) {
        /* A real short write, deliberately induced by this fixture. The owner
         * never returns to write_all to finish it. No durability is claimed. */
        size_t part = size / 2;
        if (real_write(fd, data, part) != (ssize_t)part) _exit(125);
        checkpoint(fd, "partial");
        _exit(126);
    }
    checkpoint(fd, "before");
    return real_write(fd, data, size);
}

int fsync(int fd) {
    int (*real_fsync)(int) = dlsym(RTLD_NEXT, "fsync");
    if (!real_fsync) _exit(124);
    int result = real_fsync(fd);
    if (result == 0) checkpoint(fd, "after");
    return result;
}
