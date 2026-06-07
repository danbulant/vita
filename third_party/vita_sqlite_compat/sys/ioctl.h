#ifndef MPVRS_VITA_SQLITE_COMPAT_IOCTL_H
#define MPVRS_VITA_SQLITE_COMPAT_IOCTL_H

#if defined(__has_include_next)
#  if __has_include_next(<sys/ioctl.h>)
#    include_next <sys/ioctl.h>
#  else
#    define ioctl(...) (-1)
#  endif
#else
#  define ioctl(...) (-1)
#endif

#endif
