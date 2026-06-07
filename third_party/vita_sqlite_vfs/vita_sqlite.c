#include "sqlite3.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <psp2/io/fcntl.h>
#include <psp2/io/stat.h>
#include <psp2/kernel/threadmgr.h>
#include <psp2/kernel/threadmgr/mutex.h>
#include <psp2/rtc.h>

#ifndef SCE_SEEK_SET
#define SCE_SEEK_SET 0
#endif

#ifndef SCE_KERNEL_MUTEX_ATTR_RECURSIVE
#define SCE_KERNEL_MUTEX_ATTR_RECURSIVE 0x00000002U
#endif

typedef struct VitaFile {
    sqlite3_file base;
    SceUID fd;
    int deleteOnClose;
    char path[512];
} VitaFile;

typedef struct VitaMutex {
    SceUID uid;
    int isStatic;
} VitaMutex;

#ifndef SQLITE_MUTEX_STATIC_MAIN
#define SQLITE_MUTEX_STATIC_MAIN 2
#endif
#ifndef SQLITE_MUTEX_STATIC_VFS3
#define SQLITE_MUTEX_STATIC_VFS3 13
#endif

static VitaMutex g_staticMutexes[14];

static int vita_mutex_init(void) {
    for (int i = 0; i < 14; i++) {
        if (g_staticMutexes[i].uid < 0) {
            continue;
        }
        if (g_staticMutexes[i].uid == 0) {
            char name[32];
            snprintf(name, sizeof(name), "mpvrs-sqlite-%02d", i);
            g_staticMutexes[i].uid = sceKernelCreateMutex(name, SCE_KERNEL_MUTEX_ATTR_RECURSIVE, 0, NULL);
            g_staticMutexes[i].isStatic = 1;
            if (g_staticMutexes[i].uid < 0) {
                return SQLITE_ERROR;
            }
        }
    }
    return SQLITE_OK;
}

static int vita_mutex_end(void) {
    for (int i = 0; i < 14; i++) {
        if (g_staticMutexes[i].uid > 0) {
            sceKernelDeleteMutex(g_staticMutexes[i].uid);
        }
        memset(&g_staticMutexes[i], 0, sizeof(g_staticMutexes[i]));
    }
    return SQLITE_OK;
}

static sqlite3_mutex *vita_mutex_alloc(int type) {
    if (type >= SQLITE_MUTEX_STATIC_MAIN && type <= SQLITE_MUTEX_STATIC_VFS3) {
        int idx = type - SQLITE_MUTEX_STATIC_MAIN;
        return (sqlite3_mutex *)&g_staticMutexes[idx];
    }

    VitaMutex *mutex = (VitaMutex *)malloc(sizeof(VitaMutex));
    if (!mutex) {
        return NULL;
    }
    memset(mutex, 0, sizeof(*mutex));
    mutex->uid = sceKernelCreateMutex("mpvrs-sqlite-dyn", SCE_KERNEL_MUTEX_ATTR_RECURSIVE, 0, NULL);
    if (mutex->uid < 0) {
        free(mutex);
        return NULL;
    }
    return (sqlite3_mutex *)mutex;
}

static void vita_mutex_free(sqlite3_mutex *p) {
    VitaMutex *mutex = (VitaMutex *)p;
    if (!mutex || mutex->isStatic) {
        return;
    }
    if (mutex->uid > 0) {
        sceKernelDeleteMutex(mutex->uid);
    }
    free(mutex);
}

static void vita_mutex_enter(sqlite3_mutex *p) {
    VitaMutex *mutex = (VitaMutex *)p;
    if (mutex && mutex->uid > 0) {
        sceKernelLockMutex(mutex->uid, 1, NULL);
    }
}

static int vita_mutex_try(sqlite3_mutex *p) {
    VitaMutex *mutex = (VitaMutex *)p;
    if (!mutex || mutex->uid <= 0) {
        return SQLITE_BUSY;
    }
    return sceKernelTryLockMutex(mutex->uid, 1) >= 0 ? SQLITE_OK : SQLITE_BUSY;
}

static void vita_mutex_leave(sqlite3_mutex *p) {
    VitaMutex *mutex = (VitaMutex *)p;
    if (mutex && mutex->uid > 0) {
        sceKernelUnlockMutex(mutex->uid, 1);
    }
}

static int vita_mutex_held(sqlite3_mutex *p) {
    (void)p;
    return 1;
}

static int vita_mutex_notheld(sqlite3_mutex *p) {
    (void)p;
    return 1;
}

static sqlite3_mutex_methods vita_mutex_methods = {
    vita_mutex_init,
    vita_mutex_end,
    vita_mutex_alloc,
    vita_mutex_free,
    vita_mutex_enter,
    vita_mutex_try,
    vita_mutex_leave,
    vita_mutex_held,
    vita_mutex_notheld,
};

static int vita_xClose(sqlite3_file *pFile) {
    VitaFile *p = (VitaFile*)pFile;
    if (p->fd >= 0) {
        sceIoClose(p->fd);
        p->fd = -1;
    }
    if (p->deleteOnClose && p->path[0]) {
        sceIoRemove(p->path);
    }
    return SQLITE_OK;
}

static int vita_xRead(sqlite3_file *pFile, void *zBuf, int iAmt, sqlite_int64 iOfst) {
    VitaFile *p = (VitaFile*)pFile;
    memset(zBuf, 0, iAmt);
    if (sceIoLseek(p->fd, iOfst, SCE_SEEK_SET) < 0) {
        return SQLITE_IOERR_READ;
    }
    int read = sceIoRead(p->fd, zBuf, iAmt);
    if (read == iAmt) {
        return SQLITE_OK;
    }
    if (read >= 0) {
        return SQLITE_IOERR_SHORT_READ;
    }
    return SQLITE_IOERR_READ;
}

static int vita_xWrite(sqlite3_file *pFile, const void *zBuf, int iAmt, sqlite_int64 iOfst) {
    VitaFile *p = (VitaFile*)pFile;
    if (sceIoLseek(p->fd, iOfst, SCE_SEEK_SET) < 0) {
        return SQLITE_IOERR_WRITE;
    }
    int written = sceIoWrite(p->fd, zBuf, iAmt);
    if (written != iAmt) {
        return SQLITE_IOERR_WRITE;
    }
    return SQLITE_OK;
}

static int vita_xTruncate(sqlite3_file *pFile, sqlite_int64 size) {
    VitaFile *p = (VitaFile*)pFile;
    if (sceIoChstatByFd(p->fd, &(SceIoStat){ .st_size = size }, SCE_CST_SIZE) < 0) {
        return SQLITE_IOERR_TRUNCATE;
    }
    return SQLITE_OK;
}

static int vita_xSync(sqlite3_file *pFile, int flags) {
    (void)pFile;
    (void)flags;
    return SQLITE_OK;
}

static int vita_xFileSize(sqlite3_file *pFile, sqlite_int64 *pSize) {
    VitaFile *p = (VitaFile*)pFile;
    SceIoStat stat;
    memset(&stat, 0, sizeof(stat));
    if (sceIoGetstatByFd(p->fd, &stat) < 0) {
        return SQLITE_IOERR_FSTAT;
    }
    *pSize = stat.st_size;
    return SQLITE_OK;
}

static int vita_xLock(sqlite3_file *pFile, int eLock) {
    (void)pFile;
    (void)eLock;
    return SQLITE_OK;
}

static int vita_xUnlock(sqlite3_file *pFile, int eLock) {
    (void)pFile;
    (void)eLock;
    return SQLITE_OK;
}

static int vita_xCheckReservedLock(sqlite3_file *pFile, int *pResOut) {
    (void)pFile;
    *pResOut = 0;
    return SQLITE_OK;
}

static int vita_xFileControl(sqlite3_file *pFile, int op, void *pArg) {
    (void)pFile;
    (void)op;
    (void)pArg;
    return SQLITE_NOTFOUND;
}

static int vita_xSectorSize(sqlite3_file *pFile) {
    (void)pFile;
    return 4096;
}

static int vita_xDeviceCharacteristics(sqlite3_file *pFile) {
    (void)pFile;
    return 0;
}

static int vita_xOpen(sqlite3_vfs *vfs, const char *name, sqlite3_file *file, int flags, int *outFlags) {
    (void)vfs;
    static const sqlite3_io_methods vitaio = {
        1,
        vita_xClose,
        vita_xRead,
        vita_xWrite,
        vita_xTruncate,
        vita_xSync,
        vita_xFileSize,
        vita_xLock,
        vita_xUnlock,
        vita_xCheckReservedLock,
        vita_xFileControl,
        vita_xSectorSize,
        vita_xDeviceCharacteristics,
    };

    VitaFile *p = (VitaFile*)file;
    memset(p, 0, sizeof(*p));
    p->fd = -1;
    p->deleteOnClose = (flags & SQLITE_OPEN_DELETEONCLOSE) != 0;

    if (!name) {
        return SQLITE_CANTOPEN;
    }

    unsigned oflags = 0;
    if (flags & SQLITE_OPEN_READONLY) {
        oflags |= SCE_O_RDONLY;
    } else {
        oflags |= SCE_O_RDWR;
    }
    if (flags & SQLITE_OPEN_CREATE) {
        oflags |= SCE_O_CREAT;
    }
    if (flags & SQLITE_OPEN_EXCLUSIVE) {
        oflags |= SCE_O_EXCL;
    }
    if ((flags & SQLITE_OPEN_MAIN_JOURNAL) && !(flags & SQLITE_OPEN_EXCLUSIVE)) {
        oflags |= SCE_O_CREAT;
    }

    p->fd = sceIoOpen(name, oflags, 0777);
    if (p->fd < 0 && (flags & SQLITE_OPEN_READWRITE)) {
        int roFlags = (oflags & ~(SCE_O_RDWR | SCE_O_CREAT | SCE_O_EXCL)) | SCE_O_RDONLY;
        p->fd = sceIoOpen(name, roFlags, 0);
        if (p->fd >= 0 && outFlags) {
            *outFlags = SQLITE_OPEN_READONLY;
        }
    } else if (outFlags) {
        *outFlags = flags;
    }

    if (p->fd < 0) {
        return SQLITE_CANTOPEN;
    }

    strncpy(p->path, name, sizeof(p->path) - 1);
    p->base.pMethods = &vitaio;
    return SQLITE_OK;
}

static int vita_xDelete(sqlite3_vfs *vfs, const char *name, int syncDir) {
    (void)vfs;
    (void)syncDir;
    int ret = sceIoRemove(name);
    return ret < 0 ? SQLITE_IOERR_DELETE : SQLITE_OK;
}

static int vita_xAccess(sqlite3_vfs *vfs, const char *name, int flags, int *pResOut) {
    (void)vfs;
    (void)flags;
    SceIoStat stat;
    memset(&stat, 0, sizeof(stat));
    *pResOut = sceIoGetstat(name, &stat) >= 0;
    return SQLITE_OK;
}

static int vita_xFullPathname(sqlite3_vfs *vfs, const char *zName, int nOut, char *zOut) {
    (void)vfs;
    sqlite3_snprintf(nOut, zOut, "%s", zName);
    return SQLITE_OK;
}

static void *vita_xDlOpen(sqlite3_vfs *vfs, const char *zFilename) {
    (void)vfs;
    (void)zFilename;
    return NULL;
}

static void vita_xDlError(sqlite3_vfs *vfs, int nByte, char *zErrMsg) {
    (void)vfs;
    if (nByte > 0) {
        zErrMsg[0] = 0;
    }
}

static void (*vita_xDlSym(sqlite3_vfs *vfs, void *p, const char *zSymbol))(void) {
    (void)vfs;
    (void)p;
    (void)zSymbol;
    return NULL;
}

static void vita_xDlClose(sqlite3_vfs *vfs, void *p) {
    (void)vfs;
    (void)p;
}

static int vita_xRandomness(sqlite3_vfs *vfs, int nByte, char *zOut) {
    (void)vfs;
    SceDateTime now;
    SceRtcTick tick;
    memset(&now, 0, sizeof(now));
    memset(&tick, 0, sizeof(tick));
    if (sceRtcGetCurrentClock(&now, 0) >= 0) {
        sceRtcGetTick(&now, &tick);
    } else {
        time_t t = time(NULL);
        memcpy(&tick, &t, sizeof(t) < sizeof(tick) ? sizeof(t) : sizeof(tick));
    }
    unsigned char *bytes = (unsigned char *)&tick;
    for (int i = 0; i < nByte; i++) {
        zOut[i] = (char)(bytes[i % sizeof(tick)] ^ i);
    }
    return nByte;
}

static int vita_xSleep(sqlite3_vfs *vfs, int microseconds) {
    (void)vfs;
    sceKernelDelayThread(microseconds);
    return microseconds;
}

static int vita_xCurrentTime(sqlite3_vfs *vfs, double *pTime) {
    (void)vfs;
    time_t t = 0;
    SceDateTime time;
    memset(&time, 0, sizeof(time));
    sceRtcGetCurrentClock(&time, 0);
    sceRtcGetTime_t(&time, &t);
    *pTime = ((double)t / 86400.0) + 2440587.5;
    return SQLITE_OK;
}

static int vita_xGetLastError(sqlite3_vfs *vfs, int e, char *err) {
    (void)vfs;
    (void)e;
    (void)err;
    return 0;
}

static sqlite3_vfs vita_vfs = {
    1,
    sizeof(VitaFile),
    512,
    NULL,
    "vita",
    NULL,
    vita_xOpen,
    vita_xDelete,
    vita_xAccess,
    vita_xFullPathname,
    vita_xDlOpen,
    vita_xDlError,
    vita_xDlSym,
    vita_xDlClose,
    vita_xRandomness,
    vita_xSleep,
    vita_xCurrentTime,
    vita_xGetLastError,
};

int mpvrs_sqlite_configure_mutex(void) {
    return sqlite3_config(SQLITE_CONFIG_MUTEX, &vita_mutex_methods);
}

int sqlite3_os_init(void) {
    sqlite3_vfs_register(&vita_vfs, 1);
    return SQLITE_OK;
}

int sqlite3_os_end(void) {
    return SQLITE_OK;
}
