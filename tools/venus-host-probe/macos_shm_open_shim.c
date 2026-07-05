#ifdef __APPLE__
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

static void bridgevm_shm_path(const char *name, char *out, size_t out_len) {
  const char *tmpdir = getenv("TMPDIR");
  if (!tmpdir || !tmpdir[0])
    tmpdir = "/tmp";

  char safe[128];
  size_t j = 0;
  for (size_t i = 0; name && name[i] && j + 1 < sizeof(safe); i++) {
    char c = name[i];
    if ((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
        (c >= '0' && c <= '9') || c == '.' || c == '_' || c == '-') {
      safe[j++] = c;
    } else {
      safe[j++] = '_';
    }
  }
  safe[j] = '\0';

  snprintf(out, out_len, "%s/bridgevm-venus-shm-%d-%s", tmpdir, getuid(),
           safe[0] ? safe : "anon");
}

static int bridgevm_shm_open(const char *name, int oflag, mode_t mode) {
  if (!name || name[0] != '/') {
    errno = EINVAL;
    return -1;
  }

  char path[PATH_MAX];
  bridgevm_shm_path(name, path, sizeof(path));

#ifdef O_CLOEXEC
  oflag |= O_CLOEXEC;
#endif
  return open(path, oflag, mode);
}

static int bridgevm_shm_unlink(const char *name) {
  if (!name || name[0] != '/') {
    errno = EINVAL;
    return -1;
  }

  char path[PATH_MAX];
  bridgevm_shm_path(name, path, sizeof(path));
  return unlink(path);
}

__attribute__((used)) static struct {
  const void *replacement;
  const void *replacee;
} bridgevm_interposers[] __attribute__((section("__DATA,__interpose"))) = {
    {(const void *)bridgevm_shm_open, (const void *)shm_open},
    {(const void *)bridgevm_shm_unlink, (const void *)shm_unlink},
};
#else
int bridgevm_macos_shm_open_shim_noop;
#endif
