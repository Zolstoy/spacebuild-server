use crate::error::Error;
use crate::instance::Instance;
use crate::service::Service;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
extern crate scopeguard;
use crate::spacebuild_log;

pub fn run<T>(stream: T, instance: Arc<Mutex<Instance>>, address: SocketAddr)
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + std::marker::Unpin + std::marker::Send + 'static,
{
    let io = TokioIo::new(stream);
    tokio::task::spawn(async move {
        let instance = Arc::clone(&instance);

        let result = http1::Builder::new()
            .serve_connection(
                io,
                service_fn(move |req: Request<hyper::body::Incoming>| {
                    let instance = Arc::clone(&instance);
                    serve(req, instance, address)
                }),
            )
            .with_upgrades()
            .await;
        if result.is_err() {
            spacebuild_log!(warn, address, "Serve http connection error: {}", result.err().unwrap());
        } else {
            spacebuild_log!(info, address, "Served http connection");
        }
    });
}

pub async fn serve(
    mut request: Request<hyper::body::Incoming>,
    instance: Arc<Mutex<Instance>>,
    address: SocketAddr,
) -> hyper::Result<Response<Full<Bytes>>> {
    let response_body = Full::<Bytes>::new("".into());
    let mut response = Response::<Full<Bytes>>::new(response_body);
    *response.status_mut() = StatusCode::BAD_REQUEST;

    if hyper_tungstenite::is_upgrade_request(&request) {
        spacebuild_log!(info, address, "Upgrade request");
        let res = hyper_tungstenite::upgrade(&mut request, None);
        if res.is_err() {
            let err_str: String = res.err().unwrap().to_string();

            *response.body_mut() = Full::<Bytes>::new(format!("Can't upgrade to websocket: {}", err_str).into());

            spacebuild_log!(info, address, "WS upgrade error");
            return Ok(response);
        }

        let (ws_resp, websocket) = res.unwrap();

        tokio::spawn(async move {
            let instance_cln = Arc::clone(&instance);
            spacebuild_log!(trace, address, "Waiting websocket handshake");
            let websocket = websocket.await.map_err(|_err| Error::WebSocketError);
            spacebuild_log!(trace, address, "Handshake done");
            if websocket.is_err() {
                *response.body_mut() = Full::<Bytes>::new(format!("Error").into());
                spacebuild_log!(trace, address, "websocket await error");
                return ();
            }
            let mut client = Service::new(websocket.unwrap(), instance_cln, address);
            let result = client.serve().await;
            if let Err(err) = result {
                spacebuild_log!(warn, address, "Error from client service: {}", err);
            }
        });

        return Ok(ws_resp);
    } else {
        *response.body_mut() = Full::<Bytes>::new(format!("Websocket only").into());
        spacebuild_log!(info, address, "HTTP non WS request");
        return Ok(response);
    }
}
