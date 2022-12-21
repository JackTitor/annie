#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod core;
mod mute_control;
mod tray_application;
mod window;
mod window_listener;

use std::{env, fs::File, panic, path::PathBuf, sync::mpsc};

use log::{info, LevelFilter};
use msgbox::IconType;
use simplelog::{ColorChoice, Config, TermLogger, TerminalMode, WriteLogger};

use crate::core::AnnieCore;

fn main() {
    set_panic_hook();
    setup_logger();

    info!("Annie start");

    let (core_sender, core_receiver) = mpsc::channel();
    let (_tray_thread, tray_sender) = tray_application::create_tray_thread(core_sender.clone());
    let listener_thread = window_listener::WindowListenerHandle::spawn(core_sender);

    let config_path = get_data_dir().join("annie.json");

    AnnieCore::run_with_config(config_path, core_receiver, tray_sender, listener_thread);

    info!("Annie exit");
}

fn get_data_dir() -> PathBuf {
    if cfg!(debug_assertions) {
        let mut path = env::current_exe().expect("cannot locate current exe");
        path.pop();
        path
    } else {
        dirs::document_dir().expect("cannot locate documents dir")
    }
}

fn setup_logger() {
    if cfg!(debug_assertions) {
        TermLogger::init(
            LevelFilter::Debug,
            Config::default(),
            TerminalMode::Stderr,
            ColorChoice::Auto,
        )
        .expect("cannot initialize debug mode logger");
    } else {
        let log_path = get_data_dir().join("annie.log");
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
