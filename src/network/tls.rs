use crate::error::Error;
use crate::Result;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::PrivateKeyDer;
use rustls::pki_types::PrivatePkcs8KeyDer;
use rustls::RootCertStore;
use rustls_pki_types::pem::PemObject;
use std::sync::Arc;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::TlsConnector;

pub enum ServerPki<'a> {
    Paths { key: String, cert: String },
    Slices { key: &'a [u8], cert: &'a [u8] },
    Rustls(rustls::ServerConfig),
}

#[derive(Clone)]
pub enum ClientPki<'a> {
    WebPki,
    Path { cert: String },
    Slice { cert: &'a [u8] },
    Rustls(rustls::ClientConfig),
}

pub fn get_acceptor(pki: ServerPki) -> Result<TlsAcceptor> {
    match pki {
        ServerPki::Slices { key, cert } => {
            let cert =
                CertificateDer::from_pem_slice(cert).map_err(|err| Error::CertLoadError(err))?;
            let key =
                PrivatePkcs8KeyDer::from_pem_slice(key).map_err(|err| Error::KeyLoadError(err))?;

            let config = rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![cert], PrivateKeyDer::Pkcs8(key))
                .map_err(|err| Error::TlsConfigBuildError(err))?;
            Ok(TlsAcceptor::from(TlsAcceptor::from(Arc::new(config))))
        }
        ServerPki::Paths {
            key: key_path,
            cert: cert_path,
        } => {
            let cert = CertificateDer::from_pem_file(cert_path)
                .map_err(|err| Error::CertLoadError(err))?;
            let key = PrivatePkcs8KeyDer::from_pem_file(key_path)
                .map_err(|err| Error::KeyLoadError(err))?;

            let config = rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![cert], PrivateKeyDer::Pkcs8(key))
                .map_err(|err| Error::TlsConfigBuildError(err))?;
            Ok(TlsAcceptor::from(TlsAcceptor::from(Arc::new(config))))
        }
        ServerPki::Rustls(config) => Ok(TlsAcceptor::from(Arc::new(config))),
    }
}

pub fn get_connector(pki: ClientPki) -> Result<TlsConnector> {
    match pki {
        ClientPki::Slice { cert } => {
            let mut root_store = RootCertStore::empty();
            root_store.add_parsable_certificates(
                CertificateDer::pem_slice_iter(cert).map(|result| result.unwrap()),
            );

            let config = rustls::ClientConfig::builder()
                .with_root_certificates(Arc::new(root_store))
                .with_no_client_auth();

            Ok(TlsConnector::from(Arc::new(config)))
        }
        ClientPki::Path { cert: cert_path } => {
            let mut root_store = RootCertStore::empty();
            root_store.add_parsable_certificates(
                CertificateDer::pem_file_iter(cert_path)
                    .expect("Cannot open CA file")
                    .map(|result| result.unwrap()),
            );

            let config = rustls::ClientConfig::builder()
                .with_root_certificates(Arc::new(root_store))
                .with_no_client_auth();

            Ok(TlsConnector::from(Arc::new(config)))
        }
        ClientPki::WebPki => {
            let root_store = RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.into(),
            };

            let config = rustls::ClientConfig::builder()
                .with_root_certificates(Arc::new(root_store))
                .with_no_client_auth();

            Ok(TlsConnector::from(Arc::new(config)))
        }
        ClientPki::Rustls(config) => Ok(TlsConnector::from(Arc::new(config))),
    }
}
