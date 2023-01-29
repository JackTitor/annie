use std::{
    mem,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{Receiver, Sender},
};

use flexstr::SharedStr;
use itertools::Itertools;
use log::{debug, error, info};
use unicase::UniCase;
use winapi::{
    shared::{minwindef::DWORD, windef::HWND},
    um::{
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        processthreadsapi::OpenProcess,
        tlhelp32::{
            CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32,
            TH32CS_SNAPPROCESS,
        },
        winbase::QueryFullProcessImageNameW,
        winnt::PROCESS_QUERY_INFORMATION,
    },
};

use crate::{
    config::AnnieConfig,
    error::{AnnieError, AnnieResult},
    mute_control::MuteProxy,
    tray_application::{TrayEvent, TraySender},
    window::Window,
    window_listener::WindowListenerHandle,
};

#[derive(Debug)]
pub enum CoreMessage {
    NewForegroundWindow(usize), // can't send raw hwnd
    SetEnabledGlobal(bool),
    SetEnabledApp(ProgramPath, bool),
    OpenConfig,
    ReloadConfig,
    ForceUnmuteAll,
    ExitApplication,
}

pub type CoreSender = Sender<CoreMessage>;
pub type ProgramPath = UniCase<SharedStr>;

pub struct AnnieCore {
    config: AnnieConfig,
    config_path: PathBuf,
    foreground_window: Option<Window>,
    receiver: Receiver<CoreMessage>,
    tray_sender: TraySender,
    listener_thread: Option<WindowListenerHandle>,
    mute_proxy_: Option<MuteProxy>,
}

impl AnnieCore {
    pub fn run_with_config(
        config_path: impl AsRef<Path>,
        receiver: Receiver<CoreMessage>,
        tray_sender: TraySender,
        listener_thread: WindowListenerHandle,
    ) -> Result<(), AnnieError> {
        let mut core = AnnieCore {
            config: AnnieConfig::new_empty(),
            config_path: config_path.as_ref().into(),
            foreground_window: None,
            receiver,
            tray_sender,
            listener_thread: Some(listener_thread),
            mute_proxy_: Some(MuteProxy::new()),
        };

        if !config_path.as_ref().exists() {
            core.save_config()?;
        };

        core.reload_config()?;

        // process messages (until ExitApplication is encountered)
        loop {
            let message = core
                .receiver
                .recv()
                .expect("all core senders closed, did a thread crash?");

            if !core.process_message(message)? {
                break;
            }
        }

        // join listener thread
        core.listener_thread
            .take()
            .expect("listener thread is empty")
            .join()
            .expect("cannot join listener thread");

        // join mute proxy
        core.mute_proxy_
            .take()
            .expect("")
            .join()
            .expect("cannot join mute proxy");

        Ok(())
    }

    fn mute_proxy(&self) -> &MuteProxy {
        self.mute_proxy_.as_ref().expect("mute proxy is missing")
    }

    fn is_managed(&self, program_path: &ProgramPath) -> bool {
        self.config.managed_apps.contains(program_path)
    }

    fn process_message(&mut self, message: CoreMessage) -> AnnieResult<bool> {
        let keep_processing = !matches!(&message, CoreMessage::ExitApplication);

        debug!("Core received message: {:?}", &message);

        match message {
            CoreMessage::NewForegroundWindow(hwnd) => self.handle_new_window(hwnd),
            CoreMessage::SetEnabledGlobal(enabled) => self.set_enabled_global(enabled)?,
            CoreMessage::SetEnabledApp(app_name, enabled) => {
                self.set_managed_app(app_name, enabled)?;
            }
            CoreMessage::OpenConfig => self.show_config()?,
            CoreMessage::ReloadConfig => self.reload_config()?,
            CoreMessage::ForceUnmuteAll => self.force_unmute_all(),
            CoreMessage::ExitApplication => self.exit_app(),
        }

        Ok(keep_processing)
    }

    fn handle_new_window(&mut self, hwnd: usize) {
        let window_new = match Window::new_from_hwnd(hwnd as HWND) {
            Ok(w) => w,
            Err(_) => return,
        };

        let window_old =
            self.foreground_window
                .replace(match Window::new_from_hwnd(hwnd as HWND) {
                    Ok(w) => w,
                    Err(_) => return,
                });
        let (pid_old, program_path_old) = match window_old {
            Some(Window {
                hwnd: _,
                pid,
                program_path,
            }) => (Some(pid), Some(program_path)),
            None => (None, None),
        };
        let is_managed_old = program_path_old
            .as_ref()
            .map(|path| self.is_managed(path))
            .unwrap_or(false);

        let is_managed_new = self.is_managed(&window_new.program_path);

        // mute old window, unmute new window (if managed)

        if self.config.enabled && pid_old != Some(window_new.pid) {
            if is_managed_old {
                if let Some(pid_old) = pid_old {
                    self.mute_proxy().mute(pid_old);
                }
            }

            if is_managed_new {
                self.mute_proxy().unmute(window_new.pid, true);
            }
        }

        // if the program has changed, send as recent program to tray

        if !window_new.program_path.starts_with("C:\\Windows\\")
            && Some(&window_new.program_path) != program_path_old.as_ref()
        {
            self.tray_sender
                .send_event(TrayEvent::AddRecentApp(
                    window_new.program_path.clone(),
                    is_managed_new,
                ))
                .map_err(|err| error!("Cannot send to tray: {}", err))
                .ok();
        }

        debug!("New foreground window: {:?}", &window_new);
        self.foreground_window = Some(window_new);
    }

    fn set_enabled_global(&mut self, enabled: bool) -> AnnieResult<()> {
        if enabled == self.config.enabled {
            return Ok(());
        }

        self.config.enabled = enabled;
        self.save_config()?;

        if enabled {
            self.update_mute_status_all();
        } else {
            self.force_unmute_all();
        }

        Ok(())
    }

    fn set_managed_app(&mut self, program_path: ProgramPath, managed: bool) -> AnnieResult<()> {
        if managed && self.config.managed_apps.insert(program_path.clone()) {
            // update mute status on all processes with this path
            info!("Added {} to managed apps", &program_path);

            let foreground_pid = self.foreground_window.as_ref().map(|w| w.pid);

            for pid in Self::get_pids_from_path(&program_path) {
                if Some(pid) == foreground_pid {
                    self.mute_proxy().unmute(pid, false);
                } else {
                    self.mute_proxy().mute(pid);
                }
            }
        } else if !managed && self.config.managed_apps.remove(&program_path) {
            // unmute every process with this path
            info!("Removed {} from managed apps", &program_path);
            for pid in Self::get_pids_from_path(&program_path) {
                self.mute_proxy().unmute(pid, false);
            }
        }

        self.save_config()?;

        Ok(())
    }

    fn show_config(&self) -> AnnieResult<()> {
        // explorer returns exit code 1 for some reason
        Command::new("explorer")
            .arg(format!("/select,{}", self.config_path.display()))
            .output()
            .map_err(|source| AnnieError::ShowConfigError {
                source,
                path: self.config_path.clone(),
            })?;

        Ok(())
    }

    fn save_config(&self) -> AnnieResult<()> {
        self.config
            .save_to_file(&self.config_path)
            .map_err(|source| AnnieError::SaveConfigError {
                source,
                path: self.config_path.clone(),
            })?;
        info!("Updated config file");
        debug!("{:?}", self.config);
        Ok(())
    }

    fn reload_config(&mut self) -> AnnieResult<()> {
        self.config = AnnieConfig::load_from_file(&self.config_path).map_err(|source| {
            AnnieError::LoadConfigError {
                source,
                path: self.config_path.clone(),
            }
        })?;

        self.force_unmute_all();

        self.tray_sender
            .send_event(TrayEvent::UpdateFromConfig {
                enabled: self.config.enabled,
                managed_apps: self.config.managed_apps.clone(),
                max_recent_apps: self.config.max_recent_apps,
            })
            .map_err(|err| error!("Cannot send to tray: {}", err))
            .ok();

        info!("Loaded config from file");
        debug!("{:?}", self.config);
        Ok(())
    }

    fn force_unmute_all(&self) {
        let all_windows = Window::all_windows();
        let mut pids = all_windows.into_iter().map(|w| w.pid).collect_vec();
        pids.sort_unstable();
        pids.dedup();

        for pid in pids {
            self.mute_proxy().unmute(pid, false)
        }
    }

    fn exit_app(&self) {}

    fn update_mute_status_all(&self) {
        let all_windows = Window::all_windows();
        let foreground_pid = self.foreground_window.as_ref().map(|win| win.pid);
        let pids = all_windows.into_iter().map(|w| w.pid);

        for pid in pids {
            if Some(pid) == foreground_pid {
                self.mute_proxy().unmute(pid, false);
            } else {
                self.mute_proxy().mute(pid);
            }
        }
    }

    fn get_pids_from_path(program_path: &ProgramPath) -> Vec<DWORD> {
        unsafe {
            let mut pids = vec![];

            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                error!("Failed to retrieve process snapshot");
                return pids;
            }

            let mut process_entry: PROCESSENTRY32 = mem::zeroed();
            process_entry.dwSize = mem::size_of::<PROCESSENTRY32>() as _;
            let mut hresult = Process32First(snapshot, &mut process_entry);

            let target_path = program_path.as_str().encode_utf16().collect_vec();
            let mut process_path_buf = [0u16; 1024];

            while hresult > 0 {
                let handle = OpenProcess(PROCESS_QUERY_INFORMATION, 0, process_entry.th32ProcessID);
                let mut buf_size: DWORD = process_path_buf.len() as _;
                let ok = QueryFullProcessImageNameW(
                    handle,
                    0,
                    process_path_buf.as_mut_ptr(),
                    &mut buf_size,
                );
                if ok > 0 && target_path == process_path_buf[..(buf_size as _)] {
                    pids.push(process_entry.th32ProcessID);
                }
                CloseHandle(handle);
                hresult = Process32Next(snapshot, &mut process_entry);
            }

            pids
        }
    }
}

impl Drop for AnnieCore {
    fn drop(&mut self) {
        self.force_unmute_all();
    }
}
