// Trusted PID 1, statically linked into the platform initramfs. No image code
// executes before the independent runtime is mounted and its loader selected.
#define _GNU_SOURCE
#include <sys/mount.h>
#include <sys/reboot.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <fcntl.h>
#include <termios.h>
#include <unistd.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void shutdown_vm(int code) {
    printf("G5_GUEST_EXIT=%d\n", code); fflush(stdout);
    // Flush the emulated UART before ACPI poweroff discards queued tty bytes.
    tcdrain(STDOUT_FILENO);
    sync(); reboot(RB_POWER_OFF); _exit(code);
}
static void fail(const char *what) {
    int error=errno;
    int kmsg=open("/dev/kmsg",O_WRONLY|O_CLOEXEC);
    if(kmsg>=0) { dprintf(kmsg,"<3>g5-init: %s: %s\n",what,strerror(error)); close(kmsg); }
    errno=error; perror(what); shutdown_vm(125);
}
static void directory(const char *path, mode_t mode) {
    if (mkdir(path, mode) && errno != EEXIST) fail(path);
    struct stat st;
    if (lstat(path, &st) || !S_ISDIR(st.st_mode)) { errno=EINVAL; fail(path); }
}
static void mount_fs(const char *source,const char *path,const char *kind,
                     unsigned long flags,const char *data) {
    directory(path,0755);
    if (mount(source,path,kind,flags,data)) fail(path);
}
int main(void) {
    if (getpid()!=1) return 125;
    umask(022);
    mount_fs("devtmpfs","/dev","devtmpfs",0,NULL);
    // Hosts with nested virtualization on expose KVM; the guest has no udev,
    // so make the node usable by the workload user and --device containers.
    // Guest root already belongs to the owner, so the mode restricts nothing.
    // Absent when the host keeps nested virtualization off.
    if (chmod("/dev/kvm",0666) && errno!=ENOENT) fail("/dev/kvm");
    // The minimal initramfs has no device nodes before devtmpfs is mounted.
    // Establish all standard descriptors explicitly before spawning bootstrap.
    int console=open("/dev/console",O_RDWR|O_NOCTTY);
    if(console<0) fail("console");
    for(int fd=0;fd<3;fd++) if(dup2(console,fd)<0) fail("console dup");
    if(console>2) close(console);
    setvbuf(stdout,NULL,_IOLBF,0);
    mount_fs("proc","/proc","proc",0,NULL);
    mount_fs("sysfs","/sys","sysfs",0,NULL);
    mount_fs("/dev/vda","/root","ext4",0,NULL);
    directory("/root/opt",0755);
    mount_fs("/dev/vdb","/root/opt/g5","ext4",MS_RDONLY,"noload");
    mount_fs("tmpfs","/root/run","tmpfs",MS_NOSUID|MS_NODEV,"mode=755");
    mount_fs("/dev/vdc","/root/run/g5-bootstrap","ext4",MS_RDONLY|MS_NOSUID|MS_NODEV|MS_NOEXEC,"noload");
    // Host cache profile (ontology g5.runtime.cache.profile): the shared runtime
    // disk ships only an empty placeholder; the host publishes its own profile on
    // this VM's bootstrap disk. Bind it over the placeholder now, before any view
    // of /opt/g5 is built, so the workload and its containers read the profile at
    // the runtime path, still read-only. No profile (a host without a door) leaves
    // the placeholder, which consumers treat as no profile. A profile the runtime
    // cannot receive, or one that cannot be inspected, is an incompatible input:
    // fail rather than boot a guest that silently loses its cache.
    struct stat profile;
    if (stat("/root/run/g5-bootstrap/cache-profile.env",&profile)==0) {
        if (!S_ISREG(profile.st_mode)) { errno=EINVAL; fail("cache profile"); }
        if (mount("/root/run/g5-bootstrap/cache-profile.env","/root/opt/g5/cache-profile.env",NULL,MS_BIND,NULL))
            fail("cache profile");
    } else if (errno!=ENOENT) fail("cache profile");
    mount_fs("devtmpfs","/root/dev","devtmpfs",0,NULL);
    mount_fs("proc","/root/proc","proc",0,NULL);
    mount_fs("sysfs","/root/sys","sysfs",0,NULL);
    // Move the guest disk onto the mount namespace root before entering it, as
    // switch_root does. Joining a mount namespace (setns) lands on that root, so
    // a mere chroot would leave every later namespace join in the empty
    // initramfs; see the workload bind below. The oracle
    // scripts/test-guest-init-mount-roots.py replays this sequence.
    if (chdir("/root") || mount(".","/",NULL,MS_MOVE,NULL) || chroot(".") || chdir("/")) fail("switch root");
    // Existing images select overlay2 by querying this exact mount point.
    directory("/var",0755); directory("/var/lib",0755); directory("/var/lib/docker",0755);
    if (mount("/var/lib/docker","/var/lib/docker",NULL,MS_BIND,NULL)) fail("Docker data mount");
    mount_fs("devpts","/dev/pts","devpts",MS_NOSUID|MS_NOEXEC,"gid=5,mode=620,ptmxmode=666");
    mount_fs("tmpfs","/dev/shm","tmpfs",MS_NOSUID|MS_NODEV,"mode=1777,size=1G");
    mount_fs("cgroup2","/sys/fs/cgroup","cgroup2",0,NULL);
    if (symlink("/proc/self/fd","/dev/fd") && errno!=EEXIST) fail("/dev/fd");
    const char *names[]={"/dev/stdin","/dev/stdout","/dev/stderr"};
    for (int i=0;i<3;i++) { char target[32]; snprintf(target,sizeof(target),"/proc/self/fd/%d",i); if (symlink(target,names[i])&&errno!=EEXIST) fail(names[i]); }
    // A platform-owned bootstrap root keeps image loader, crypto, NSS and
    // interpreter configuration out of recovery. Only /work is writable input.
    mount_fs("tmpfs","/run/g5-trusted","tmpfs",MS_NOSUID|MS_NODEV,"mode=755,size=64M");
    const char *base="/run/g5-trusted";
    const char *dirs[]={"etc","tmp","root","opt","opt/g5","run","run/g5-bootstrap","work","image","dev","proc","sys"};
    for(unsigned int i=0;i<sizeof(dirs)/sizeof(dirs[0]);i++) {
        char path[128]; snprintf(path,sizeof(path),"%s/%s",base,dirs[i]); directory(path,0755);
    }
    if(mount("/opt/g5","/run/g5-trusted/opt/g5",NULL,MS_BIND,NULL)
       || mount("/run/g5-bootstrap","/run/g5-trusted/run/g5-bootstrap",NULL,MS_BIND,NULL)
       || mount("/work","/run/g5-trusted/work",NULL,MS_BIND,NULL)
       || mount("/dev","/run/g5-trusted/dev",NULL,MS_BIND|MS_REC,NULL)
       || mount("/proc","/run/g5-trusted/proc",NULL,MS_BIND,NULL)
       || mount("/sys","/run/g5-trusted/sys",NULL,MS_BIND|MS_REC,NULL)) fail("bootstrap mounts");
    // Do not recursively bind / here: /run contains this new bootstrap root.
    // Add the handful of workload mounts needed by customer processes explicitly.
    if(mount("/","/run/g5-trusted/image",NULL,MS_BIND,NULL)) fail("workload root bind");
    const char *mounts[]={"/opt/g5","/run","/dev","/proc","/sys","/var/lib/docker"};
    for(unsigned int i=0;i<sizeof(mounts)/sizeof(mounts[0]);i++) {
        char path[128]; snprintf(path,sizeof(path),"%s/image%s",base,mounts[i]);
        // /run must not recurse back into the bootstrap root.
        unsigned long flags=MS_BIND | (strcmp(mounts[i],"/run") ? MS_REC : 0);
        if(mount(mounts[i],path,NULL,flags,NULL)) fail("workload mount bind");
    }
    // Stack the workload view on the namespace root. The bootstrap keeps using
    // it at /image; the workload enters it by joining PID 1's mount namespace
    // (g5d vm-init), which resolves to the top of this stack. Containers the
    // workload starts pivot_root from there, so `docker exec`, Compose health
    // checks and `nsenter -m` land in the container instead of the initramfs.
    if(mount("/run/g5-trusted/image","/",NULL,MS_BIND|MS_REC,NULL)) fail("workload root");
    pid_t child=fork(); if(child<0) fail("fork bootstrap");
    if(child==0) {
        if(chdir(base) || chroot(".") || chdir("/")) fail("bootstrap root");
        char *argv[]={"/opt/g5/bootstrap/loader","--inhibit-cache","--library-path","/opt/g5/bootstrap/libs",
                      "/opt/g5/bin/g5d","vm-init",NULL};
        char *env[]={"PATH=/opt/g5/bootstrap:/opt/g5/bin","HOME=/root","LANG=C",NULL};
        execve(argv[0],argv,env); fail("trusted bootstrap exec");
    }
    for (;;) {
        int status; pid_t pid=waitpid(-1,&status,0);
        if(pid<0) { if(errno==EINTR) continue; fail("wait bootstrap"); }
        if(pid==child) shutdown_vm(WIFEXITED(status)?WEXITSTATUS(status):128+WTERMSIG(status));
    }
}
