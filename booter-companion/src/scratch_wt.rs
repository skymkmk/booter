use std::sync::Arc;
use rustls::client::{ServerCertVerifier, ServerCertVerified};
use rustls::{Certificate, Error, ServerName};
use std::time::SystemTime;

struct DangerVerifier;
impl ServerCertVerifier for DangerVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &Certificate,
        _intermediates: &[Certificate],
        _server_name: &ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: SystemTime,
    ) -> Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }
}

fn test() {
    let crypto = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_custom_certificate_verifier(Arc::new(DangerVerifier))
        .with_no_client_auth();
}
