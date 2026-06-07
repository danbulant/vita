use vitasdk_sys::sceKernelChangeThreadPriority;

extern "C" {
    fn sceKernelGetThreadCurrentPriority() -> i32;
    fn sceKernelGetThreadId() -> i32;
}

pub const AUDIO_THREAD_PRIORITY: i32 = 144;
pub const ARTWORK_THREAD_PRIORITY: i32 = 191;

pub fn set_current_thread_priority(label: &str, priority: i32) {
    unsafe {
        let thread_id = sceKernelGetThreadId();
        let before = sceKernelGetThreadCurrentPriority();
        let result = sceKernelChangeThreadPriority(thread_id, priority);
        let after = sceKernelGetThreadCurrentPriority();
        eprintln!(
            "mpvrs: thread priority {label}: id={thread_id} before={before} requested={priority} result={result} after={after}"
        );
    }
}
