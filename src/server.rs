use crate::error::Error;
use crate::http;
use crate::instance::Instance;
use crate::spacebuild_log;
use crate::tls;
use crate::tls::ClientPki;
use crate::tls::ServerPki;
use crate::Result;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

pub enum InstanceConfig {
    UserInstance(Arc<Mutex<Instance>>),
    UserSqliteDb { path: String },
}

pub enum TcpConfig {
    Port(u16),
    TcpListener(TcpListener),
}

pub struct ServerConfig<'a> {
    pub tcp: TcpConfig,
    pub pki: Option<ServerPki<'a>>,
}

pub struct ClientConfig<'a> {
    pub addr: String,
    pub nickname: String,
    pub pki: ClientPki<'a>,
}

pub async fn run(
    instance_config: InstanceConfig,
    server_config: ServerConfig<'_>,
    stop: crossbeam::channel::Receiver<()>,
) -> Result<()> {
    let instance = match instance_config {
        InstanceConfig::UserInstance(instance) => instance,
        InstanceConfig::UserSqliteDb { path } => {
            spacebuild_log!(info, "server", "Loading {}", path);
            Arc::new(Mutex::new(Instance::from_path(path.as_str()).await?))
        }
    };

    let listener = match server_config.tcp {
        TcpConfig::Port(port) => TcpListener::bind(format!("localhost:{}", port))
            .await
            .map_err(|err| Error::TcpCouldNotConnect(err))?,
        TcpConfig::TcpListener(listener) => listener,
    };

    let tls_acceptor = if let Some(pki) = server_config.pki {
        Some(tls::get_acceptor(pki)?)
    } else {
        None
    };

    let mut ref_instant = tokio::time::Instant::now();
    let tick_value = std::time::Duration::from_millis(100);
    let mut update_tick_delay = tokio::time::interval(tick_value);
    let mut save_tick_delay = tokio::time::interval(std::time::Duration::from_secs(30));

    spacebuild_log!(
        info,
        "server",
        "Server loop starts, listenning on {}",
        listener.local_addr().unwrap().port()
    );

    save_tick_delay.tick().await;

    loop {
        tokio::select! {
            // ----------------------------------------------------
            // ON UPDATE TICK DELAY--------------------------------
            now = update_tick_delay.tick() => {

                let mut must_stop = false;
                if stop.try_recv().is_ok() {
                    spacebuild_log!(info, "server", "Stop signal received");
                    must_stop = true;
                }

                let delta = now - ref_instant;
                if delta > tick_value {
                    spacebuild_log!(warn, "server", "Server loop is too slow: {}s", delta.as_secs_f64());
                }
                ref_instant = now;
                instance.lock().await.update(delta.as_secs_f64()).await;

                if must_stop{
                    instance.lock().await.save_all().await;
                    spacebuild_log!(info, "server", "Server loop stops now (on stop channel)!");
                    return Ok(())
                }
            },
            // ----------------------------------------------------
            // ON SAVE TICK DELAY----------------------------------
            _ = save_tick_delay.tick() => {

                instance.lock().await.save_all().await;
            },
            // ----------------------------------------------------
            // ON TCP ACCEPT---------------------------------------
            Ok((stream, addr)) = listener.accept() => {
                spacebuild_log!(info, "server", "TCP accept from: {}", addr);

                let cln = Arc::clone(&instance);
                if let Some(tls_acceptor) = tls_acceptor.clone() {
                    let acceptor = tls_acceptor.clone();
                    tokio::spawn(async move {
                        let tls_stream = acceptor.accept(stream).await.map_err(|_err| Error::FailedTlsHandshake);
                        if tls_stream.is_err() {
                            spacebuild_log!(warn, "server", "TLS accept error: {}", tls_stream.is_err());
                        } else {
                            http::run(tls_stream.unwrap(), cln, addr);
                        }

                    });
                } else {
                    http::run(stream, Arc::clone(&instance), addr);
                }
            },
        }
    }
}
