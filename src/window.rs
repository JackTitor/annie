use std::{
    error::Error,
    fmt::{Debug, Display},
};

use comedy::Win32Error;

use unicase::UniCase;
use winapi::{
    shared::{
        minwindef::{BOOL, DWORD, FALSE, LPARAM, TRUE},
        ntdef::WCHAR,
        windef::HWND,
    },
    um::{
        processthreadsapi::OpenProcess,
        winbase::QueryFullProcessImageNameW,
        winnt::PROCESS_QUERY_INFORMATION,
        winuser::{
            EnumWindows, GetWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
            GW_OWNER,
        },
    },
};

use crate::core::ProgramPath;

pub struct WindowError {
    hwnd: usize,
    reason: &'static str,
}

impl WindowError {
    fn new(hwnd: HWND, reason: &'static str) -> Self {
        Self {
            hwnd: hwnd as usize,
            reason,
        }
    }
}

impl Display for WindowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {:#x}", self.reason, self.hwnd)
    }
}

impl Debug for WindowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {:#x}", self.reason, self.hwnd)
    }
}

impl Error for WindowError {}

#[derive(Debug)]
pub struct Window {
    pub hwnd: HWND,
    pub pid: DWORD,
    pub program_path: ProgramPath,
}

impl Window {
    pub fn new_from_hwnd(hwnd: HWND) -> anyhow::Result<Self> {
        unsafe {
            if GetWindow(hwnd, GW_OWNER) != 0 as _ {
                Err(WindowError::new(hwnd, "Window has an owner"))?;
            }

            // get pid
            let mut pid: DWORD = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 {
                Err(WindowError::new(
                    hwnd,
                    "Could not retrieve process ID window",
                ))?;
            }

            // check that window is visible
            if IsWindowVisible(hwnd) == FALSE {
                Err(WindowError::new(hwnd, "Window is not visible"))?;
            }

            // check that window has a title
            const MAX_TITLE_SIZE: usize = 16;
            let mut buf: [WCHAR; MAX_TITLE_SIZE] = [0; MAX_TITLE_SIZE];
            let title_len = GetWindowTextW(hwnd, buf.as_mut_ptr(), MAX_TITLE_SIZE as i32);
            if title_len == 0 {
                Err(WindowError::new(hwnd, "Window has empty title"))?;
            }

            // get program name

            let hproc = OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid);
            if hproc == 0 as _ {
                Err(Win32Error::get_last_error())?;
            }

            let mut path_buf = [0u16; 1024];
            let mut buf_size: DWORD = 1024;
            let ok = QueryFullProcessImageNameW(hproc, 0, path_buf.as_mut_ptr(), &mut buf_size);
            if ok == FALSE {
                Err(Win32Error::get_last_error())?;
            }

            let program_path = String::from_utf16(&path_buf[..(buf_size as _)])?;
            let program_path = UniCase::new(program_path.into());

            // ok

            Ok(Self {
                hwnd,
                pid,
                program_path,
            })
        }
    }

    pub fn all_windows() -> Vec<Window> {
        unsafe extern "system" fn enumerate_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
            if let Ok(window) = Window::new_from_hwnd(hwnd) {
                let windows = lparam as *mut Vec<Window>;
                (*windows).push(window);
            }

            TRUE
        }

        unsafe {
            let mut windows: Vec<Window> = vec![];
            EnumWindows(
                Some(enumerate_callback),
                &mut windows as *mut Vec<Window> as _,
            );
            windows
        }
    }
}
