// Copyright 2019-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

const HELP: &str = "\
USAGE: tauri-driver [OPTIONS]

OPTIONS:
  -h, --help              Prints help information
  --port NUMBER           Sets the tauri-driver intermediary port (default: 4444)
  --native-port NUMBER    Sets the port of the plugin WebDriver (default: 4445)
  --native-host HOST      Sets the host of the plugin WebDriver (default: 127.0.0.1)

PLATFORM NOTES:
  All platforms use tauri-plugin-webdriver (embedded in app).
";

#[derive(Debug, Clone)]
pub struct Args {
    pub port: u16,
    pub native_port: u16,
    pub native_host: String,
}

impl From<pico_args::Arguments> for Args {
    fn from(mut args: pico_args::Arguments) -> Self {
        // if the user wanted help, we don't care about parsing the rest of the args
        if args.contains(["-h", "--help"]) {
            println!("{HELP}");
            std::process::exit(0);
        }

        let parsed = Args {
            port: args.value_from_str("--port").unwrap_or(4444),
            native_port: args.value_from_str("--native-port").unwrap_or(4445),
            native_host: args
                .value_from_str("--native-host")
                .unwrap_or(String::from("127.0.0.1")),
        };

        // be strict about accepting args, error for anything extraneous
        let rest = args.finish();
        if !rest.is_empty() {
            eprintln!("Error: unused arguments left: {rest:?}");
            eprintln!("{HELP}");
            std::process::exit(1);
        }

        parsed
    }
}
