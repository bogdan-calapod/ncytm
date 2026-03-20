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
        .subcommands([clap::Command::new("info").about("Print platform information like paths")])
}
