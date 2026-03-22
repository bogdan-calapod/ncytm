use clap::builder::PathBufValueParser;

pub const AUTHOR: &str = "ncytm contributors";
pub const BIN_NAME: &str = "ncytm";
pub const CONFIGURATION_FILE_NAME: &str = "config.toml";
pub const USER_STATE_FILE_NAME: &str = "userstate.cbor";

/// Return the [Command](clap::Command) that models the program's command line arguments. The
/// command can be used to parse the actual arguments passed to the program, or to automatically
/// generate a man page using clap's mangen package.
pub fn program_arguments() -> clap::Command {
    // TODO: Add audio backends info once we have a player implementation
    let backends = "Audio backends: rodio (planned)";

    clap::Command::new("ncytm")
        .version(env!("VERSION"))
        .author(AUTHOR)
        .about("cross-platform ncurses YouTube Music client")
        .after_help(backends)
        .arg(
            clap::Arg::new("debug")
                .short('d')
                .long("debug")
                .value_name("FILE")
                .value_parser(PathBufValueParser::new())
                .help("Enable debug logging to the specified file"),
        )
        .arg(
            clap::Arg::new("basepath")
                .short('b')
                .long("basepath")
                .value_name("PATH")
                .value_parser(PathBufValueParser::new())
                .help("custom basepath to config/cache files"),
        )
        .arg(
            clap::Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Filename of config file in basepath")
                .default_value(CONFIGURATION_FILE_NAME),
        )
        .subcommands([
            clap::Command::new("info").about("Print platform information like paths"),
            clap::Command::new("auth")
                .about("Manage YouTube Music authentication")
                .arg(
                    clap::Arg::new("browser")
                        .long("browser")
                        .action(clap::ArgAction::SetTrue)
                        .help("Launch a browser to authenticate with YouTube Music"),
                )
                .arg(
                    clap::Arg::new("no-system-profile")
                        .long("no-system-profile")
                        .action(clap::ArgAction::SetTrue)
                        .help("Use a separate ncytm browser profile instead of system profile"),
                )
                .arg(
                    clap::Arg::new("browser-type")
                        .long("browser-type")
                        .value_name("TYPE")
                        .value_parser(["chrome", "edge", "chromium"])
                        .default_value("edge")
                        .help("Browser to use: chrome, edge, or chromium"),
                )
                .arg(
                    clap::Arg::new("check")
                        .long("check")
                        .action(clap::ArgAction::SetTrue)
                        .help("Check if current cookies are valid"),
                )
                .arg(
                    clap::Arg::new("timeout")
                        .long("timeout")
                        .value_name("SECONDS")
                        .value_parser(clap::value_parser!(u64))
                        .default_value("600")
                        .help("Timeout for browser authentication in seconds"),
                ),
        ])
}
