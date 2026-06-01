use lettre::{Message, AsyncSmtpTransport, AsyncTransport, transport::smtp::authentication::Credentials};
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::Tokio1Executor;
use crate::config::SmtpConfig;

pub async fn send_otp_email(config: &SmtpConfig, to_email: &str, otp: &str) -> Result<(), String> {
    let email = Message::builder()
        .from(config.user.parse().map_err(|e| format!("Invalid from address: {}", e))?)
        .to(to_email.parse().map_err(|e| format!("Invalid to address: {}", e))?)
        .subject("Your Booter Login Code")
        .body(format!("Your verification code is: {}\n\nThis code is valid for 5 minutes.", otp))
        .map_err(|e| e.to_string())?;

    let creds = Credentials::new(config.user.clone(), config.pass.clone());

    let tls_params = TlsParameters::new(config.host.clone())
        .map_err(|e| format!("TLS init error: {}", e))?;

    let tls = if config.port == 465 {
        Tls::Wrapper(tls_params)
    } else {
        Tls::Opportunistic(tls_params)
    };

    let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
        .map_err(|e| format!("SMTP relay init failed: {}", e))?
        .port(config.port)
        .tls(tls)
        .credentials(creds)
        .build();

    mailer.send(email).await.map_err(|e| format!("SMTP send failed: {}", e))?;
    
    Ok(())
}
