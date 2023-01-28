#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod core;
mod error;
mod mute_control;
mod tray_application;
mod window;
mod window_listener;

use std::{
    env,
    fs::{self, File},
    panic,
    path::{Path, PathBuf},
    sync::mpsc,
};

use log::{info, LevelFilter};
use msgbox::IconType;
use simplelog::{ColorChoice, Config, TermLogger, TerminalMode, WriteLogger};
use single_instance::SingleInstance;

use crate::core::AnnieCore;

fn main() {
    let Some(_instance_lock) = get_instance_lock() else { return };

    set_panic_hook();

    let data_dir = get_or_create_data_dir();
    let config_path = data_dir.join("annie.toml");

    setup_logger(&data_dir);

    info!("Annie start");

    let (core_sender, core_receiver) = mpsc::channel();
    let (_tray_thread, tray_sender) = tray_application::create_tray_thread(core_sender.clone());
    let listener_thread = window_listener::WindowListenerHandle::spawn(core_sender);

    AnnieCore::run_with_config(config_path, core_receiver, tray_sender, listener_thread).unwrap();

    info!("Annie exit"); // TODO: This is not reached - why?
}

fn get_instance_lock() -> Option<SingleInstance> {
    let instance = SingleInstance::new("annie-da43c2e0bb1f724e650535165731ecac").unwrap();

    if !instance.is_single() {
        std::mem::drop(instance);
        msgbox::create("Annie", "Annie is already running.", IconType::Info).ok();

        None
    } else {
        Some(instance)
    }
}

fn get_or_create_data_dir() -> PathBuf {
    if cfg!(debug_assertions) {
        let mut path = env::current_exe().expect("cannot locate current exe");
        path.pop();
        path
    } else {
        let mut path = dirs::data_local_dir().expect("cannot locate LocalAppData");
        path.push("annie");
        fs::create_dir_all(&path).expect("cannot create data dir");
        path
    }
}

fn setup_logger(data_dir: impl AsRef<Path>) {
    if cfg!(debug_assertions) {
        TermLogger::init(
            LevelFilter::Debug,
            Config::default(),
            TerminalMode::Stderr,
            ColorChoice::Auto,
        )
        .expect("cannot initialize debug mode logger");
    } else {
        let log_path = data_dir.as_ref().join("annie_log.log");
        let log_file = File::create(log_path).expect("cannot write to log file");
        WriteLogger::init(LevelFilter::Info, Config::default(), log_file)
            .expect("cannot initialize logger");
    }
}

fn set_panic_hook() {
    if cfg!(debug_assertions) {
        let old_hook = panic::take_hook();
        panic::set_hook(Box::new(move |panic_info| {
            old_hook(panic_info);
            std::process::exit(3);
        }));
    } else {
        panic::set_hook(Box::new(|panic_info| {
            let message = panic_message::panic_info_message(panic_info);
            msgbox::create("Annie fatal error", message, IconType::Error).ok();
            std::process::exit(3);
        }));
    }
}
