use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "shrike",
    about = "Run commands in a persistent Docker container",
    long_about = None,
    disable_help_flag = false,
)]
pub struct Args {
    #[arg(
        short = 'n',
        long = "new",
        value_name = "TEMPLATE",
        help = "Generate new config file [TEMPLATE: default|cmake]"
    )]
    pub new: Option<Option<String>>,

    #[arg(short = 'l', long = "list", help = "List aliases for selected profile")]
    pub list: bool,

    #[arg(short = 'L', long = "list-profiles", help = "List all known profiles")]
    pub list_profiles: bool,

    #[arg(
        short = 'p',
        long = "profile",
        value_name = "PROFILE",
        help = "Select profile"
    )]
    pub profile: Option<String>,

    #[arg(short = 'e', long = "env", value_name = "SPEC", action = clap::ArgAction::Append,
          help = "Pass extra env vars (KEY, KEY=VAL, or KEY=$(cmd)); repeatable")]
    pub env: Vec<String>,

    #[arg(
        short = 'I',
        long = "interactive",
        help = "Force all steps interactive (docker exec -it)"
    )]
    pub interactive: bool,

    #[arg(
        short = 'r',
        long = "restart",
        help = "Remove and recreate the container"
    )]
    pub restart: bool,

    #[arg(short = 'b', long = "rebuild", help = "Force rebuild of Docker image")]
    pub rebuild: bool,

    #[arg(
        short = 's',
        long = "stop",
        help = "Stop and remove the current project's container"
    )]
    pub stop: bool,

    #[arg(
        short = 'S',
        long = "stop-all",
        help = "Stop and remove ALL shrike-managed containers"
    )]
    pub stop_all: bool,

    /// Command or alias to run, plus any arguments
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,
}
