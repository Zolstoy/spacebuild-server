extern crate tokio;

use std::time::Duration;

use clap::Parser;

use anyhow::Result;
use spacebuild::{client::Client, network::tls::ClientPki};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(version, long_about = None)]
struct Args {
    #[arg(value_name = "HOST", default_value = "localhost")]
    host: String,

    #[arg(value_name = "PORT", default_value_t = 2567)]
    port: u16,

    #[arg(short,
        long,
        value_name = "CA_CERT_PATH",
        num_args(0..=1)
    )]
    tls: Option<Option<String>>,

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
async fn main() -> Result<()> {
    let args = Args::parse();

    let capacity = 25;
    let mut handles = Vec::with_capacity(capacity);

    // common::trace::init(Some(args.trace_filter));

    let pki = if let Some(tls) = args.tls {
        if let Some(ca_cert_path) = tls {
            Some(ClientPki::Path { cert: ca_cert_path })
        } else {
            Some(ClientPki::WebPki)
        }
    } else {
        None
    };

    for _ in 0..capacity {
        let host = args.host.clone();

        let pki = pki.clone();
        handles.push(tokio::spawn(async move {
            let mut player =
                Client::connect(format!("{}:{}", host, args.port).as_str(), pki).await?;

            let new_v4 = Uuid::new_v4();
            player.login(new_v4.to_string().as_str()).await?;

            tokio::time::sleep(Duration::from_secs(10)).await;

            player.terminate().await?;

            anyhow::Ok(())
        }));
    }

    for handle in handles {
        handle.await.unwrap()?;
    }
    Ok(())
}
