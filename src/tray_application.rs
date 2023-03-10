use std::{
    borrow::Cow,
    collections::{HashSet, VecDeque},
    iter,
    path::Path,
    sync::mpsc::{self},
    thread::{self, JoinHandle},
};

use itertools::Itertools;
use log::{debug, error, info};

use msgbox::IconType;
use winit::{
    event::Event,
    event_loop::{ControlFlow, EventLoop, EventLoopProxy},
    platform::windows::EventLoopExtWindows,
};

use trayicon::{Icon, MenuBuilder, TrayIcon, TrayIconBuilder};

use crate::core::{CoreMessage, CoreSender, ProgramPath};

pub type TraySender = EventLoopProxy<TrayEvent>;

const TRAY_ICON_BLUE: &[u8] = include_bytes!("../resource/annie-small-blue.ico");
const TRAY_ICON_RED: &[u8] = include_bytes!("../resource/annie-small-red.ico");

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum TrayEvent {
    // UI events
    ToggleGlobal,
    ToggleProgram(usize),
    OpenConfig,
    ReloadConfig,
    ForceUnmuteAll,
    ShowAbout,
    Exit,
    // core events
    AddRecentApp(ProgramPath, bool),
    UpdateFromConfig {
        enabled: bool,
        managed_apps: HashSet<ProgramPath>,
        max_recent_apps: usize,
    },
}

#[derive(Clone, PartialEq, Eq, Default)]
struct TrayState {
    enabled: bool,
    recent_apps: VecDeque<(ProgramPath, bool)>,
    max_recent_apps: usize,
}

fn update_tray_app(tray_app: &mut TrayIcon<TrayEvent>, tray_state: &TrayState) {
    // recent apps submenu

    let mut recent_apps_menu = MenuBuilder::new();

    for (index, (app_path, app_active)) in tray_state.recent_apps.iter().enumerate() {
        recent_apps_menu = recent_apps_menu.checkable(
            &get_app_tray_text(app_path),
            *app_active,
            TrayEvent::ToggleProgram(index),
        );
    }

    // context menu

    let menu = MenuBuilder::new()
        .checkable("Enable muting", tray_state.enabled, TrayEvent::ToggleGlobal)
        .submenu("Recent apps", recent_apps_menu)
        .separator()
        .item("Show config file", TrayEvent::OpenConfig)
        .item("Reload config from file", TrayEvent::ReloadConfig)
        .item("Force unmute all apps", TrayEvent::ForceUnmuteAll)
        .separator()
        .item("About", TrayEvent::ShowAbout)
        .item("Exit", TrayEvent::Exit);
    tray_app.set_menu(&menu).expect("failed to set tray menu");

    // tray icon

    let icon_bytes = match tray_state.enabled {
        true => TRAY_ICON_BLUE,
        false => TRAY_ICON_RED,
    };

    tray_app
        .set_icon(&Icon::from_buffer(icon_bytes, None, None).unwrap())
        .expect("cannot update tray icon");

    // tooltip
    tray_app
        .set_tooltip(match tray_state.enabled {
            true => "Annie",
            false => "Annie (disabled)",
        })
        .expect("cannot update tray tooltip");
}

fn get_app_name(app_path: &str) -> Cow<str> {
    let name = Path::new(app_path)
        .file_stem()
        .and_then(|n| n.to_str())
        .expect("cannot extract program name from path");

    let first = name
        .chars()
        .next()
        .expect("cannot extract first character from program name");

    if Iterator::eq(first.to_uppercase(), iter::once(first)) {
        Cow::Borrowed(name)
    } else {
        let split = name
            .char_indices()
            .map(|(i, _)| i)
            .nth(1)
            .unwrap_or(name.len());

        Cow::Owned(format!("{}{}", first.to_uppercase(), &name[split..]))
    }
}

fn get_app_tray_text(app_path: &str) -> String {
    format!("{} ({})", get_app_name(app_path), app_path)
}

fn show_about_message() {
    thread::spawn(|| {
        let body = format!(
            "Annie Automuter\n\nVersion: {}\nBranch: {}\nCommit: {}\nCommit date: {}\nPlatform: {}\nProfile: {}",
            env!("VERGEN_BUILD_SEMVER"),
            env!("VERGEN_GIT_BRANCH"),
            env!("VERGEN_GIT_SHA_SHORT"),
            env!("VERGEN_GIT_COMMIT_DATE"),
            env!("VERGEN_CARGO_TARGET_TRIPLE"),
            env!("VERGEN_CARGO_PROFILE")
        );
        msgbox::create("About Annie", &body, IconType::Info).expect("cannot create message box");
    });
}

pub fn create_tray_thread(core_sender: CoreSender) -> (JoinHandle<()>, TraySender) {
    let (temp_sender, temp_receiver) = mpsc::channel();

    // annie-core immediately sends a message to update this
    let mut tray_state = TrayState {
        enabled: true,
        ..Default::default()
    };

    let thread_handle = thread::spawn(move || {
        info!("Tray start");

        let event_loop = EventLoop::<TrayEvent>::new_any_thread();
        let proxy = event_loop.create_proxy();

        temp_sender
            .send(proxy.clone())
            .expect("cannot send proxy to temp sender");
        std::mem::drop(temp_sender);

        let mut tray_app = TrayIconBuilder::new()
            .sender_winit(proxy)
            .icon_from_buffer(TRAY_ICON_BLUE)
            .tooltip("Annie")
            .menu(MenuBuilder::new())
            .build()
            .expect("could not create tray icon");

        update_tray_app(&mut tray_app, &tray_state);

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;

            if let Event::UserEvent(user_event) = event {
                debug!("Tray received event: {:?}", &user_event);

                match user_event {
                    TrayEvent::ToggleGlobal => {
                        tray_state.enabled = !tray_state.enabled;

                        core_sender
                            .send(CoreMessage::SetEnabledGlobal(tray_state.enabled))
                            .map_err(|err| error!("Cannot send to core: {}", err))
                            .ok();

                        update_tray_app(&mut tray_app, &tray_state);
                    }
                    TrayEvent::UpdateFromConfig {
                        enabled,
                        managed_apps,
                        max_recent_apps,
                    } => {
                        tray_state.enabled = enabled;

                        tray_state.max_recent_apps = max_recent_apps;

                        if tray_state.recent_apps.len() > max_recent_apps {
                            tray_state
                                .recent_apps
                                .resize(max_recent_apps, Default::default());
                        }

                        for (app_path, app_enabled) in &mut tray_state.recent_apps {
                            *app_enabled = managed_apps.contains(app_path);
                        }

                        update_tray_app(&mut tray_app, &tray_state);
                    }
                    TrayEvent::ToggleProgram(app_index) => {
                        let (app_path, app_active) = &mut tray_state.recent_apps[app_index];

                        *app_active = !*app_active;

                        core_sender
                            .send(CoreMessage::SetEnabledApp(app_path.clone(), *app_active))
                            .map_err(|err| error!("Cannot send to core: {}", err))
                            .ok();

                        update_tray_app(&mut tray_app, &tray_state);
                    }
                    TrayEvent::AddRecentApp(app_path, app_active) => {
                        let recent = &mut tray_state.recent_apps;

                        if let Some((index, _)) =
                            recent.iter().find_position(|(path, _)| path == &app_path)
                        {
                            recent.remove(index);
                        }

                        recent.push_front((app_path, app_active));

                        if recent.len() > tray_state.max_recent_apps {
                            recent.pop_back();
                        }

                        update_tray_app(&mut tray_app, &tray_state);
                    }
                    TrayEvent::OpenConfig => {
                        core_sender
                            .send(CoreMessage::OpenConfig)
                            .map_err(|err| error!("Cannot send to core: {}", err))
                            .ok();
                    }
                    TrayEvent::ReloadConfig => {
                        core_sender
                            .send(CoreMessage::ReloadConfig)
                            .map_err(|err| error!("Cannot send to core: {}", err))
                            .ok();
                    }

                    TrayEvent::ForceUnmuteAll => {
                        core_sender
                            .send(CoreMessage::ForceUnmuteAll)
                            .map_err(|err| error!("Cannot send to core: {}", err))
                            .ok();
                    }
                    TrayEvent::ShowAbout => {
                        show_about_message();
                    }
                    TrayEvent::Exit => {
                        *control_flow = ControlFlow::Exit;
                        core_sender
                            .send(CoreMessage::ExitApplication)
                            .map_err(|err| error!("Cannot send to core: {}", err))
                            .ok();
                        info!("Tray exit");
                    }
                }
            }
        });
    });

    let tray_sender = temp_receiver
        .recv()
        .expect("cannot receive proxy from temp sender");

    (thread_handle, tray_sender)
}
