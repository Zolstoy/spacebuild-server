use std::{env, io};

use clap::Parser;

use spacebuild::{
    network::tls::ServerPki,
    server::{self, InstanceConfig, ServerConfig},
};
use tokio::task::JoinHandle;

use anyhow::{bail, Result};

pub mod trace {
    use regex::Regex;
    use std::io::Write;
    use std::{env, thread};
    use tokio::time::Instant;

    const THREAD_ID_REGEX_STR: &str = "ThreadId\\(([[:digit:]]+)\\)";

    pub fn init(maybe_filter: Option<String>) {
        let launch_time = Instant::now();
        let target_regex_str = if let Some(filter) = maybe_filter {
            filter
        } else {
            let maybe_trace_filter = env::var("SPACEBUILD_TRACE_FILTER");
            if maybe_trace_filter.is_err() {
                format!("(.*)")
            } else {
                maybe_trace_filter.unwrap()
            }
        };

        let mut binding = env_logger::builder();
        let builder = binding.format(move |buf, record| {
            let target_str = record.target();
            let regex = Regex::new(target_regex_str.as_str()).unwrap();
            let mut results = vec![];
            for (_, [target]) in regex.captures_iter(target_str).map(|c| c.extract()) {
                results.push(target);
            }

            if results.len() != 1 {
                return write!(buf, "");
            }

            let target_str = results.last().unwrap();

            let thread_id_str = format!("{:?}", thread::current().id());
            let regex = Regex::new(THREAD_ID_REGEX_STR).unwrap();
            let mut results = vec![];

            for (_, [id]) in regex
                .captures_iter(thread_id_str.as_str())
                .map(|c| c.extract())
            {
                results.push(id);
            }
            assert_eq!(1, results.len());
            let thread_id_str = results.last().unwrap();

            let now_time = Instant::now();
            let elapsed = now_time - launch_time;
            let elapsed = elapsed.as_millis() as f32 / 1000.;

            let args_str = format!("{}", record.args());

            writeln!(
                buf,
                "{:<8}{:<4}{:<30}{}",
                elapsed.to_string(),
                thread_id_str,
                target_str,
                args_str,
            )
        });
        builder.init();
    }
}


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

    #[arg(short, long, default_value = "galaxy.sbdb")]
    instance: String,

    #[arg(long, default_value = "spacebuild::(.*)", value_name = "REGEX")]
    trace_filter: String,

    #[arg(
        long,
        default_value = "INFO",
        value_name = "TRACE|DEBUG|INFO|WARN|ERROR"
    )]
    trace_level: String,
}

#[tokio::main]
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

    trace::init(Some(args.trace_filter));
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
            InstanceConfig::UserSqliteDb {
                path: args.instance,
            },
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
