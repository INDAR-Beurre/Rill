/*
 * rill_appimage_preload.c — Tiny MIT-licensed LD_PRELOAD shim for Rill AppImage.
 *
 * Problem (PKG-003): WebKitGTK 4.1 compiles its helper directory (e.g.
 * "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1" on Debian, "/usr/libexec/webkit2gtk-4.1"
 * on Fedora, "/usr/lib/webkit2gtk-4.1" on Arch) into libwebkit2gtk-4.1.so.0 and
 * launches WebKitNetworkProcess / WebKitWebProcess via g_subprocess_launcher_spawnv(),
 * which calls posix_spawn() / posix_spawnp() in modern GLib — bypassing
 * libapprun_hooks.so's execve()-only hooks.
 *
 * When a Debian-built AppImage runs on Arch/Artix/Fedora/openSUSE, the host has
 * no "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1", so WebKitNetworkProcess fails
 * with ENOENT unless posix_spawn / posix_spawnp / execve redirect the compiled-in
 * WebKit helper path to $WEBKIT_EXEC_PATH inside the mounted AppDir.
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <limits.h>
#include <spawn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static const char *const WEBKIT_PREFIXES[] = {
    "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/",
    "/usr/lib/webkit2gtk-4.1/",
    "/usr/lib64/webkit2gtk-4.1/",
    "/usr/libexec/webkit2gtk-4.1/",
    NULL
};

static const char *rewrite_webkit_path(const char *path, char *buf, size_t bufsz) {
    if (!path || path[0] != '/') {
        return path;
    }
    const char *exec_dir = getenv("WEBKIT_EXEC_PATH");
    if (!exec_dir || !exec_dir[0]) {
        return path;
    }
    for (int i = 0; WEBKIT_PREFIXES[i] != NULL; ++i) {
        size_t plen = strlen(WEBKIT_PREFIXES[i]);
        if (strncmp(path, WEBKIT_PREFIXES[i], plen) == 0) {
            int n = snprintf(buf, bufsz, "%s/%s", exec_dir, path + plen);
            if (n > 0 && (size_t)n < bufsz) {
                return buf;
            }
            return path;
        }
    }
    return path;
}

int posix_spawn(pid_t *pid, const char *path,
                const posix_spawn_file_actions_t *file_actions,
                const posix_spawnattr_t *attrp,
                char *const argv[], char *const envp[]) {
    typedef int (*real_fn)(pid_t *, const char *,
                           const posix_spawn_file_actions_t *,
                           const posix_spawnattr_t *,
                           char *const[], char *const[]);
    static real_fn real_posix_spawn = NULL;
    if (!real_posix_spawn) {
        real_posix_spawn = (real_fn)dlsym(RTLD_NEXT, "posix_spawn");
    }
    char rewritten[PATH_MAX];
    const char *target = rewrite_webkit_path(path, rewritten, sizeof(rewritten));
    return real_posix_spawn(pid, target, file_actions, attrp, argv, envp);
}

int posix_spawnp(pid_t *pid, const char *file,
                 const posix_spawn_file_actions_t *file_actions,
                 const posix_spawnattr_t *attrp,
                 char *const argv[], char *const envp[]) {
    typedef int (*real_fn)(pid_t *, const char *,
                           const posix_spawn_file_actions_t *,
                           const posix_spawnattr_t *,
                           char *const[], char *const[]);
    static real_fn real_posix_spawnp = NULL;
    if (!real_posix_spawnp) {
        real_posix_spawnp = (real_fn)dlsym(RTLD_NEXT, "posix_spawnp");
    }
    char rewritten[PATH_MAX];
    const char *target = rewrite_webkit_path(file, rewritten, sizeof(rewritten));
    return real_posix_spawnp(pid, target, file_actions, attrp, argv, envp);
}

int execve(const char *pathname, char *const argv[], char *const envp[]) {
    typedef int (*real_fn)(const char *, char *const[], char *const[]);
    static real_fn real_execve = NULL;
    if (!real_execve) {
        real_execve = (real_fn)dlsym(RTLD_NEXT, "execve");
    }
    char rewritten[PATH_MAX];
    const char *target = rewrite_webkit_path(pathname, rewritten, sizeof(rewritten));
    return real_execve(target, argv, envp);
}

int execv(const char *pathname, char *const argv[]) {
    typedef int (*real_fn)(const char *, char *const[]);
    static real_fn real_execv = NULL;
    if (!real_execv) {
        real_execv = (real_fn)dlsym(RTLD_NEXT, "execv");
    }
    char rewritten[PATH_MAX];
    const char *target = rewrite_webkit_path(pathname, rewritten, sizeof(rewritten));
    return real_execv(target, argv);
}
