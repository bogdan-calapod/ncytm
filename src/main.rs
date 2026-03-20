#[macro_use]
extern crate cursive;
#[macro_use]
extern crate serde;

use std::{path::PathBuf, process::exit};

use application::{Application, setup_logging};
use config::set_configuration_base_path;
use log::error;
use ncytm::program_arguments;

mod application;
mod authentication;
mod cli;
mod command;
mod commands;
mod config;
mod events;
mod ext_traits;
mod library;
mod model;
mod panic;
mod player;
mod player_worker;
mod queue;
mod serialization;
mod sharing;
mod spotify;
mod spotify_api;
mod spotify_url;
mod spotify_worker;
mod theme;
mod traits;
mod ui;
mod utils;
mod youtube;
mod youtube_music;
mod youtube_url;

#[cfg(unix)]
mod ipc;

#[cfg(feature = "mpris")]
mod mpris;

#[cfg(feature = "media_control")]
mod media_control;

#[cfg(all(target_os = "macos", feature = "media_control"))]
mod macos_event_loop;

fn main() -> Result<(), String> {
    // Set a custom backtrace hook that writes the backtrace to a file instead of stdout, since
    // stdout is most likely in use by Cursive.
    panic::register_backtrace_panic_handler();

    // Parse the command line arguments.
    let matches = program_arguments().get_matches();

    // Enable debug logging to a file if specified on the command line.
    if let Some(filename) = matches.get_one::<PathBuf>("debug") {
        setup_logging(filename).expect("logger could not be initialized");
    }

    // Set the configuration base path. All configuration files are read/written relative to this
    // path.
    set_configuration_base_path(matches.get_one::<PathBuf>("basepath").cloned());

    match matches.subcommand() {
        Some(("info", _subcommand_matches)) => cli::info(),
        Some((_, _)) => unreachable!(),
        None => {
            // On macOS with media_control, we need to run winit on the main thread
            // and the cursive app in a worker thread
            #[cfg(all(target_os = "macos", feature = "media_control"))]
            {
                let config_path = matches.get_one::<String>("config").cloned();
                macos_event_loop::run_with_macos_event_loop(move |media_handle, media_events| {
                    run_application(config_path, Some(media_handle), Some(media_events))
                })
            }

            #[cfg(not(all(target_os = "macos", feature = "media_control")))]
            {
                run_application(matches.get_one::<String>("config").cloned(), None, None)
            }
        }
    }?;

    Ok(())
}

/// Run the application with optional media control handle (for macOS)
#[cfg(all(target_os = "macos", feature = "media_control"))]
fn run_application(
    config_path: Option<String>,
    media_handle: Option<macos_event_loop::MediaControlHandle>,
    media_events: Option<std::sync::mpsc::Receiver<macos_event_loop::MediaControlEvent>>,
) -> Result<(), String> {
    // Create the application.
    let mut application = match Application::new(config_path, media_handle, media_events) {
        Ok(application) => application,
        Err(error) => {
            eprintln!("{error}");
            error!("{error}");
            exit(-1);
        }
    };

    // Start the application event loop.
    application.run()
}

#[cfg(not(all(target_os = "macos", feature = "media_control")))]
fn run_application(
    config_path: Option<String>,
    _media_handle: Option<()>,
    _media_events: Option<()>,
) -> Result<(), String> {
    // Create the application.
    let mut application = match Application::new(config_path) {
        Ok(application) => application,
        Err(error) => {
            eprintln!("{error}");
            error!("{error}");
            exit(-1);
        }
    };

    // Start the application event loop.
    application.run()
}
