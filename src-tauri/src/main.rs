// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::Parser;
use handy_app_lib::CliArgs;

fn main() {
    let cli_args = CliArgs::parse();

    // CLI companion: when a subcommand is given, run headlessly as a client to
    // the running app's localhost server instead of launching the GUI.
    if let Some(command) = cli_args.command.clone() {
        std::process::exit(handy_app_lib::run_cli(command));
    }

    // A bare launch of the PATH-installed CLI copy (e.g. `handy` typed into
    // Start/Run) cannot start the GUI: the copy lives outside the install dir
    // and has no `resources\`. Forward to the installed app; fail open (run as
    // usual) when it can't be found.
    #[cfg(windows)]
    if let (Ok(exe), Ok(cli_path)) = (std::env::current_exe(), handy_app_lib::cli_install_path()) {
        let is_cli_copy = exe
            .to_string_lossy()
            .eq_ignore_ascii_case(cli_path.to_string_lossy().as_ref());
        if is_cli_copy {
            if let Some(installed) = handy_app_lib::installed_app_path() {
                if std::process::Command::new(&installed)
                    .args(std::env::args_os().skip(1))
                    .spawn()
                    .is_ok()
                {
                    return;
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        // DMABUF renderer causes crashes on various GPU/display server configurations
        // See: https://github.com/tauri-apps/tauri/issues/9394
        // SAFETY: called before any other thread is spawned (top of main).
        unsafe {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    handy_app_lib::run(cli_args)
}
