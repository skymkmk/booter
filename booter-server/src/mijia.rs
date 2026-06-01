use base64::{Engine as _, engine::general_purpose::STANDARD as b64};
use rand::Rng;
use rc4::{KeyInit, Rc4, StreamCipher};
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn gen_nonce() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let mut rng = rand::thread_rng();
    let rand_val = rng.r#gen::<i64>();

    let mut b = rand_val.to_be_bytes().to_vec();
    let part2 = (millis / 60000) as i32;
    b.extend_from_slice(&part2.to_be_bytes());

    b64.encode(b)
}

pub fn get_signed_nonce(ssecret: &str, nonce: &str) -> String {
    let mut hasher = Sha256::new();
    if let Ok(ssecret_bytes) = b64.decode(ssecret) {
        hasher.update(ssecret_bytes);
    }
    if let Ok(nonce_bytes) = b64.decode(nonce) {
        hasher.update(nonce_bytes);
    }
    b64.encode(hasher.finalize())
}

pub fn gen_enc_signature(
    uri: &str,
    method: &str,
    signed_nonce: &str,
    params: &[(String, String)],
) -> String {
    let mut signature_params = vec![method.to_uppercase(), uri.to_string()];

    for (k, v) in params {
        signature_params.push(format!("{}={}", k, v));
    }
    signature_params.push(signed_nonce.to_string());

    let signature_string = signature_params.join("&");

    let mut hasher = Sha1::new();
    hasher.update(signature_string.as_bytes());
    b64.encode(hasher.finalize())
}

pub fn encrypt_rc4(password: &str, payload: &str) -> String {
    let pwd_bytes = b64.decode(password).unwrap_or_default();
    let mut rc4 = Rc4::new_from_slice(&pwd_bytes).expect("Invalid key length");

    let mut dummy = vec![0u8; 1024];
    rc4.apply_keystream(&mut dummy);

    let mut data = payload.as_bytes().to_vec();
    rc4.apply_keystream(&mut data);

    b64.encode(data)
}

pub fn decrypt_rc4(password: &str, payload: &str) -> Vec<u8> {
    let pwd_bytes = b64.decode(password).unwrap_or_default();
    let mut rc4 = Rc4::new_from_slice(&pwd_bytes).expect("Invalid key length");

    let mut dummy = vec![0u8; 1024];
    rc4.apply_keystream(&mut dummy);

    let mut data = b64.decode(payload).unwrap_or_default();
    rc4.apply_keystream(&mut data);

    data
}
