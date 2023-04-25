use clap::{Parser, arg};
use lazy_static::lazy_static;


#[derive(Parser)]
pub(crate) struct EnvConfig {

    #[arg(
        long,
        env,
        help="How long to sleep for when there is not data on the internal health channel",
        default_value="5000"
    )]
    pub health_check_interval_millis: u64,

    #[arg(
        long,
        env,
        help="Network port to use for health check",
        default_value="29354"
    )]
    pub health_port: u64,
}

lazy_static! {
    pub(crate) static ref ENV: EnvConfig = EnvConfig::parse();
}
