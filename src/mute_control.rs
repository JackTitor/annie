use comedy::error::succeeded_or_err;
use log::{info, warn};
use winapi::shared::{
    minwindef::{BOOL, DWORD},
    ntdef::HRESULT,
};

extern "C" {
    fn SetApplicationMute(pid: DWORD, mute: BOOL) -> HRESULT;
}

pub fn set_mute(pid: DWORD, mute: bool) {
    unsafe {
        let hr = SetApplicationMute(pid, mute as _);
        succeeded_or_err(hr)
            .map(|_| info!("Set mute status for PID {} to {}", pid, mute))
            .map_err(|hr| match hr.try_into_win32_err() {
                Ok(err) => warn!("Could not set mute status on PID {}: {}", pid, err),
                Err(hr) => warn!("Could not set mute status on PID {}: {}", pid, hr),
            })
            .ok();
    }
}
