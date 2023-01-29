use std::{
    os::windows::prelude::AsRawHandle,
    sync::Mutex,
    thread::{self, JoinHandle},
};

use itertools::Itertools;
use log::info;
use once_cell::sync::Lazy;
use winapi::{
    shared::{
        minwindef::{DWORD, UINT},
        ntdef::LONG,
        windef::{HWINEVENTHOOK, HWND},
    },
    um::{
        combaseapi::{CoInitializeEx, CoUninitialize},
        processthreadsapi::GetThreadId,
        winuser::{
            DispatchMessageW, GetMessageW, PostQuitMessage, PostThreadMessageW, SetWinEventHook,
            TranslateMessage, UnhookWinEvent, EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND,
            WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_USER,
        },
    },
};

use crate::core::{CoreMessage, CoreSender};

const TARGET_EVENTS: &[DWORD] = &[EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND];
const TARGET_DW_FLAGS: UINT = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
const WM_STOP_LISTENING: UINT = WM_USER + 1;

// WinEventProc doesn't accept custom data -> communicate via a static
static CORE_SENDER: Lazy<Mutex<Option<CoreSender>>> = Lazy::new(|| Mutex::new(None));

unsafe fn window_listener_loop() {
    unsafe extern "system" fn window_change_callback(
        _: HWINEVENTHOOK,
        _: DWORD,
        hwnd: HWND,
        _: LONG,
        _: LONG,
        _: DWORD,
        _: DWORD,
    ) {
        let Some(lock) = CORE_SENDER.lock().ok() else { return };
        if let Some(core_sender) = &*lock {
            core_sender
                .send(CoreMessage::NewForegroundWindow(hwnd as usize))
                .ok();
        }
    }

    info!("Listener start");

    // set up hooks

    CoInitializeEx(0 as _, 0);

    let hooks = TARGET_EVENTS
        .iter()
        .map(|&target_event_id| {
            let hook = SetWinEventHook(
                target_event_id,
                target_event_id,
                0 as _,
                Some(window_change_callback),
                0,
                0,
                TARGET_DW_FLAGS,
            );

            if hook == (0 as _) {
                panic!("could not initialize hook");
            }

            hook
        })
        .collect_vec();

    // run message loop

    let mut msg = std::mem::zeroed();

    while GetMessageW(&mut msg, 0 as _, 0, 0) != 0 {
        match msg.message {
            WM_STOP_LISTENING => {
                PostQuitMessage(0);
            }
            _ => {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    // cleanup

    for hook in hooks {
        UnhookWinEvent(hook);
    }

    CoUninitialize();

    CORE_SENDER
        .lock()
        .expect("cannot lock core sender mutex")
        .take()
        .expect("sender is empty during listener exit");

    info!("Listener exit");
}

pub struct WindowListenerHandle {
    listener_thread: JoinHandle<()>,
}

impl WindowListenerHandle {
    pub fn spawn(core_sender: CoreSender) -> Self {
        let old_sender = CORE_SENDER
            .lock()
            .expect("cannot lock core sender mutex")
            .replace(core_sender);

        assert!(old_sender.is_none(), "dangling core sender");

        WindowListenerHandle {
            listener_thread: thread::spawn(|| unsafe { window_listener_loop() }),
        }
    }

    pub fn join(self) -> thread::Result<()> {
        unsafe {
            let thread_id = GetThreadId(self.listener_thread.as_raw_handle() as _);
            let success = PostThreadMessageW(thread_id, WM_STOP_LISTENING, 0, 0);
            assert!(
                success > 0,
                "failed to post WM_STOP_LISTENING to listener thread"
            );
        }

        self.listener_thread.join()
    }
}
