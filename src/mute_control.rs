use std::{
    collections::HashSet,
    mem::{self, MaybeUninit},
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
    time::{Duration, SystemTime},
};

use comedy::Win32Error;
use log::{debug, info, warn};
use winapi::{
    shared::minwindef::{BOOL, DWORD, FALSE, FILETIME},
    um::{
        handleapi::CloseHandle,
        processthreadsapi::{GetProcessTimes, OpenProcess},
        winnt::PROCESS_QUERY_LIMITED_INFORMATION,
    },
};

extern "C" {
    fn SetProcessMute(dwPID: DWORD, bMute: BOOL) -> BOOL;
}

#[derive(Debug)]
enum MuteProxyMessage {
    Mute(DWORD),
    Unmute(DWORD, bool),
    UnmuteFollowup(DWORD, SystemTime),
}

pub struct MuteProxy {
    proxy_sender: Sender<MuteProxyMessage>,
    proxy_thread: JoinHandle<()>,
}

impl MuteProxy {
    const PROCESS_AGE_THRESHOLD_MS: u128 = 5000;

    pub fn new() -> Self {
        let (proxy_sender, proxy_receiver) = mpsc::channel();
        let proxy_sender_clone = proxy_sender.clone();
        let proxy_thread =
            thread::spawn(move || Self::run_proxy(proxy_sender_clone, proxy_receiver));

        MuteProxy {
            proxy_sender,
            proxy_thread,
        }
    }

    pub fn join(self) -> thread::Result<()> {
        mem::drop(self.proxy_sender);
        self.proxy_thread.join()
    }

    pub fn mute(&self, pid: DWORD) {
        self.proxy_sender
            .send(MuteProxyMessage::Mute(pid))
            .expect("failed to send message to mute proxy");
    }

    pub fn unmute(&self, pid: DWORD, aggressive: bool) {
        self.proxy_sender
            .send(MuteProxyMessage::Unmute(pid, aggressive))
            .expect("failed to send message to mute proxy");
    }

    fn run_proxy(sender: Sender<MuteProxyMessage>, receiver: Receiver<MuteProxyMessage>) {
        info!("Mute proxy start");

        let mut currently_unmuting = HashSet::<DWORD>::new();

        while let Ok(message) = receiver.recv() {
            debug!("Mute proxy received message: {:?}", &message);

            match message {
                MuteProxyMessage::Mute(pid) => {
                    Self::set_mute_synchronous(pid, true);
                    currently_unmuting.remove(&pid);
                }
                MuteProxyMessage::Unmute(pid, aggressive) => {
                    Self::set_mute_synchronous(pid, false);

                    if aggressive && !currently_unmuting.contains(&pid) {
                        if let Some(start_time) = Self::get_process_start_time(pid) {
                            if Self::get_ms_since(start_time) < Self::PROCESS_AGE_THRESHOLD_MS {
                                info!("Process {} is newly opened, unmuting several times", pid);
                                currently_unmuting.insert(pid);
                                Self::unmute_followup_delayed(&sender, pid, start_time);
                            }
                        }
                    }
                }
                MuteProxyMessage::UnmuteFollowup(pid, start_time) => {
                    if currently_unmuting.contains(&pid) {
                        let process_age_ms = Self::get_ms_since(start_time);

                        if process_age_ms < Self::PROCESS_AGE_THRESHOLD_MS {
                            // process is not old enough -> mute and continue
                            Self::set_mute_synchronous(pid, false);
                            Self::unmute_followup_delayed(&sender, pid, start_time);
                        } else if process_age_ms < Self::PROCESS_AGE_THRESHOLD_MS + 1000 {
                            // process age is close beyond threshold -> mute and end
                            Self::set_mute_synchronous(pid, false);
                            currently_unmuting.remove(&pid);
                        } else {
                            // process is old enough -> end
                            currently_unmuting.remove(&pid);
                        }
                    }
                }
            }
        }

        info!("Mute proxy exit"); // TODO: This is not reached - why?
    }

    fn unmute_followup_delayed(
        sender: &Sender<MuteProxyMessage>,
        pid: DWORD,
        start_time: SystemTime,
    ) {
        let sender = sender.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(1000));
            sender
                .send(MuteProxyMessage::UnmuteFollowup(pid, start_time))
                .expect("failed to send message to mute proxy");
        });
    }

    fn get_process_start_time(pid: DWORD) -> Option<SystemTime> {
        unsafe {
            let hproc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
            if hproc == 0 as _ {
                warn!(
                    "Failed to get process handle for process {}: {}",
                    pid,
                    Win32Error::get_last_error()
                );

                return None;
            }

            let mut filetime: FILETIME = mem::zeroed();
            GetProcessTimes(
                hproc,
                &mut filetime,
                MaybeUninit::uninit().as_mut_ptr(),
                MaybeUninit::uninit().as_mut_ptr(),
                MaybeUninit::uninit().as_mut_ptr(),
            );

            CloseHandle(hproc);

            let time_started =
                ((filetime.dwHighDateTime as u64) << 32) + (filetime.dwLowDateTime as u64);

            Some(mem::transmute(time_started))
        }
    }

    fn get_ms_since(timestamp: SystemTime) -> u128 {
        SystemTime::now()
            .duration_since(timestamp)
            .expect("start time after end time")
            .as_millis()
    }

    fn set_mute_synchronous(pid: DWORD, mute: bool) {
        if mute {
            info!("Muting process {}", pid);
        } else {
            info!("Unmuting process {}", pid);
        }

        unsafe {
            // ignore hresult - can't do anything useful with the error anyway
            SetProcessMute(pid, mute as _);
        }
    }
}
