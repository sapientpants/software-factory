// DYLD interposer: logs path-taking file lookups (open/stat/access/opendir/...) with result.
#include <fcntl.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
#include <dirent.h>
#include <errno.h>
#include <sys/attr.h>

#define INTERPOSE(new, old) \
  __attribute__((used)) static struct { const void *n; const void *o; } _i_##old \
  __attribute__((section("__DATA,__interpose"))) = { (const void *)new, (const void *)old };

static void logit(const char *op, const char *path, int ok) {
  const char *out = getenv("FTRACE_OUT");
  if (!out || !path) return;
  int saved = errno;
  char buf[4096];
  int n = snprintf(buf, sizeof buf, "%d %s %s %s\n", getpid(), op, ok ? "OK" : "ENOENT/ERR", path);
  int fd = open(out, O_WRONLY | O_APPEND | O_CREAT, 0644);
  if (fd >= 0) { write(fd, buf, n > (int)sizeof buf ? sizeof buf : n); close(fd); }
  errno = saved;
}
int my_open(const char *p, int flags, ...) { mode_t m = 0; if (flags & O_CREAT) { va_list a; va_start(a, flags); m = va_arg(a, int); va_end(a); } int r = open(p, flags, m); logit("open", p, r >= 0); return r; }
int my_openat(int d, const char *p, int flags, ...) { mode_t m = 0; if (flags & O_CREAT) { va_list a; va_start(a, flags); m = va_arg(a, int); va_end(a); } int r = openat(d, p, flags, m); logit("openat", p, r >= 0); return r; }
int my_stat(const char *p, struct stat *s) { int r = stat(p, s); logit("stat", p, r == 0); return r; }
int my_lstat(const char *p, struct stat *s) { int r = lstat(p, s); logit("lstat", p, r == 0); return r; }
int my_fstatat(int d, const char *p, struct stat *s, int f) { int r = fstatat(d, p, s, f); logit("fstatat", p, r == 0); return r; }
int my_access(const char *p, int m) { int r = access(p, m); logit("access", p, r == 0); return r; }
DIR *my_opendir(const char *p) { DIR *r = opendir(p); logit("opendir", p, r != NULL); return r; }
int my_getattrlist(const char *p, void *al, void *ab, size_t sz, unsigned int o) { int r = getattrlist(p, al, ab, sz, o); logit("getattrlist", p, r == 0); return r; }
ssize_t my_readlink(const char *p, char *b, size_t n) { ssize_t r = readlink(p, b, n); logit("readlink", p, r >= 0); return r; }
INTERPOSE(my_open, open)
INTERPOSE(my_openat, openat)
INTERPOSE(my_stat, stat)
INTERPOSE(my_lstat, lstat)
INTERPOSE(my_fstatat, fstatat)
INTERPOSE(my_access, access)
INTERPOSE(my_opendir, opendir)
INTERPOSE(my_getattrlist, getattrlist)
INTERPOSE(my_readlink, readlink)
