#define _GNU_SOURCE
#include <fcntl.h>
#include <stdarg.h>
#include <string.h>
#include <unistd.h>
#include <dlfcn.h>
#include <time.h>

int clock_gettime(clockid_t clock, struct timespec *value) {
    if (clock == CLOCK_MONOTONIC) {
        value->tv_sec = 1;
        value->tv_nsec = 0;
        return 0;
    }
    int (*system_clock)(clockid_t, struct timespec *) = dlsym(RTLD_NEXT, "clock_gettime");
    return system_clock(clock, value);
}

static void report(const char *path) {
    if (strstr(path, ".pangloss-diagnostic-")) {
        static const char text[] = "diagnostic test: temporary opened\n";
        write(STDERR_FILENO, text, sizeof(text) - 1);
    }
}

int open64(const char *path, int flags, ...) {
    int (*original)(const char *, int, ...) = dlsym(RTLD_NEXT, "open64");
    report(path);
    if ((flags & O_CREAT) || (flags & O_TMPFILE) == O_TMPFILE) {
        va_list ap;
        va_start(ap, flags);
        mode_t mode = va_arg(ap, mode_t);
        va_end(ap);
        return original(path, flags, mode);
    }
    return original(path, flags);
}

int openat64(int directory, const char *path, int flags, ...) {
    int (*original)(int, const char *, int, ...) = dlsym(RTLD_NEXT, "openat64");
    report(path);
    if ((flags & O_CREAT) || (flags & O_TMPFILE) == O_TMPFILE) {
        va_list ap;
        va_start(ap, flags);
        mode_t mode = va_arg(ap, mode_t);
        va_end(ap);
        return original(directory, path, flags, mode);
    }
    return original(directory, path, flags);
}
