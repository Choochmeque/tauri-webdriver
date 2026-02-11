// Copyright 2019-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

#[cfg(not(target_os = "macos"))]
use crate::cli::Args;
#[cfg(not(target_os = "macos"))]
use std::process::Command;

// the name of the binary to find in $PATH
#[cfg(target_os = "linux")]
const DRIVER_BINARY: &str = "WebKitWebDriver";

#[cfg(target_os = "windows")]
const DRIVER_BINARY: &str = "msedgedriver.exe";

/// Find the native driver binary in the PATH, or exits the process with an error.
#[cfg(any(target_os = "linux", windows))]
pub fn native(args: &Args) -> Command {
    let native_binary = match args.native_driver.as_deref() {
        Some(custom) => {
            if custom.exists() {
                custom.to_owned()
            } else {
                eprintln!(
                    "can not find the supplied binary path {}. This is currently required.",
                    custom.display()
                );
                match std::env::current_dir() {
                    Ok(cwd) => eprintln!("current working directory: {}", cwd.display()),
                    Err(error) => eprintln!("can not find current working directory: {error}"),
                }
                std::process::exit(1);
            }
        }
        None => match which::which(DRIVER_BINARY) {
            Ok(binary) => binary,
            Err(error) => {
                eprintln!(
                    "can not find binary {DRIVER_BINARY} in the PATH. This is currently required.\
          You can also pass a custom path with --native-driver"
                );
                eprintln!("{error:?}");
                std::process::exit(1);
            }
        },
    };

    let mut cmd = Command::new(native_binary);
    cmd.env("TAURI_AUTOMATION", "true"); // 1.x
    cmd.env("TAURI_WEBVIEW_AUTOMATION", "true"); // 2.x
    cmd.arg(format!("--port={}", args.native_port));
    cmd.arg(format!("--host={}", args.native_host));

    // Prevent msedgedriver and its child processes (WebView2) from
    // creating visible console windows during testing.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    cmd
}
