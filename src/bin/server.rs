use anyhow::{bail, Result};
use clap::Parser;
use spacebuild::{
    server::{self, InstanceConfig, ServerConfig},
    tls::ServerPki,
    tracing,
};
use std::{env, io};
use tokio::task::JoinHandle;

#[derive(Parser, Debug)]
#[command(version, long_about = None)]
struct Args {
    #[arg(value_name = "PORT", default_value_t = 2567)]
    port: u16,

    #[arg(short, long,
        num_args = 2,
        value_names = ["CERT_PATH", "KEY_PATH"],
    )]
    tls: Option<Vec<String>>,

    #[arg(short, long, default_value = "galaxy.db")]
    instance: String,

    #[arg(long, default_value = "spacebuild::(.*)", value_name = "REGEX")]
    trace_filter: String,

    #[arg(long, default_value = "INFO", value_name = "TRACE|DEBUG|INFO|WARN|ERROR")]
    trace_level: String,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    env::set_var("RUST_LOG", args.trace_level);
    let pki = if let Some(tls) = args.tls {
        Some(ServerPki::Paths {
            cert: tls.first().unwrap().clone(),
            key: tls.last().unwrap().clone(),
        })
    } else {
        None
    };

    tracing::init(Some(args.trace_filter));
    let (stop_on_input_send, stop_on_input_recv) = crossbeam::channel::bounded(1);
    tokio::spawn(async move {
        loop {
            for line in io::stdin().lines() {
                if let Ok(line) = line {
                    if line == "stop" {
                        stop_on_input_send.send(()).unwrap();
                        return;
                    }
                }
            }
        }
    });

    let server_hdl: JoinHandle<Result<()>> = tokio::spawn(async move {
        if let spacebuild::Result::Err(err) = server::run(
            InstanceConfig::UserSqliteDb { path: args.instance },
            ServerConfig {
                tcp: server::TcpConfig::Port(args.port),
                pki,
            },
            stop_on_input_recv,
        )
        .await
        {
            bail!(format!("Server error: {}", err))
        } else {
            Ok(())
        }
    });

    server_hdl.await??;

    Ok(())
}
