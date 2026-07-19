//! Keep-awake (Batch 2 F6): stops Windows from sleeping while the server is
//! online, via SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED).
//! Bound with a tiny extern "system" declaration -- kernel32 is always
//! present, no new crate needed (the `windows` crate is not a dependency of
//! this project; checked Cargo.toml before writing this).
//!
//! ES_CONTINUOUS state is PER-THREAD: it lives and dies with the thread that
//! set it. Tauri commands run on pool threads that can be recycled, which
//! would silently drop the request -- so all calls are funneled through one
//! dedicated, long-lived manager thread that owns the state. Idempotent:
//! re-sending the current state is a no-op at the OS level. On process exit
//! the thread dies and Windows clears the request automatically; lib.rs
//! additionally sends an explicit clear from the RunEvent::Exit handler.

#[cfg(windows)]
mod imp {
    use std::sync::mpsc::Sender;
    use std::sync::{Mutex, OnceLock};

    const ES_CONTINUOUS: u32 = 0x8000_0000;
    const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetThreadExecutionState(es_flags: u32) -> u32;
    }

    static TX: OnceLock<Mutex<Sender<bool>>> = OnceLock::new();

    pub fn set(on: bool) {
        let tx = TX.get_or_init(|| {
            let (tx, rx) = std::sync::mpsc::channel::<bool>();
            std::thread::spawn(move || {
                for on in rx {
                    let flags =
                        if on { ES_CONTINUOUS | ES_SYSTEM_REQUIRED } else { ES_CONTINUOUS };
                    // Returns the previous state (0 on failure) -- nothing
                    // actionable either way, so the result is ignored.
                    unsafe {
                        SetThreadExecutionState(flags);
                    }
                }
                // Sender dropped (process shutting down): leave the state
                // cleared; the OS would clear it on thread exit regardless.
                unsafe {
                    SetThreadExecutionState(ES_CONTINUOUS);
                }
            });
            Mutex::new(tx)
        });
        // A poisoned lock or dead receiver just means shutdown is underway --
        // the OS clears the state then anyway.
        if let Ok(guard) = tx.lock() {
            let _ = guard.send(on);
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn set(_on: bool) {}
}

pub fn keep_awake(on: bool) {
    imp::set(on);
}
