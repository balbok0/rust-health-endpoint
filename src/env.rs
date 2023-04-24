use clap::{Parser, arg};
use lazy_static::lazy_static;


#[derive(Parser)]
pub(crate) struct EnvConfig {

    #[arg(long, env, help="")]
    pub health_check_interval_millis: u64,

    #[arg(long, env, help="")]
    pub health_port: u64,
}

lazy_static! {
    pub(crate) static ref ENV: EnvConfig = EnvConfig::parse();
}
