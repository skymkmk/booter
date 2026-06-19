#![allow(dead_code)]

use crate::mijia::{decrypt_rc4, encrypt_rc4, gen_enc_signature, gen_nonce, get_signed_nonce};
use flate2::read::GzDecoder;
use rand::{Rng, distributions::Alphanumeric};
use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::{CONTENT_TYPE, COOKIE, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MijiaAuthData {
    pub user_id: String,
    pub c_user_id: String,
    pub service_token: String,
    pub ssecurity: String,
    pub pass_token: Option<String>,
    pub psecurity: Option<String>,
    pub expire_time: i64,
    #[serde(default, alias = "deviceId")]
    pub device_id: String,
    #[serde(default)]
    pub pass_o: String,
    #[serde(default, alias = "ua")]
    pub user_agent: String,
}

pub struct MijiaClient {
    client: reqwest::Client,
    cookie_jar: Arc<Jar>,
    pub device_id: String,
    pub pass_o: String,
    pub user_agent: String,
    pub auth_data: Option<MijiaAuthData>,
    auth_updated: bool,
}

impl MijiaClient {
    pub fn new(device_id: String, pass_o: String, user_agent: String) -> Self {
        let cookie_jar = Arc::new(Jar::default());
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .cookie_provider(cookie_jar.clone())
            .build()
            .unwrap();

        Self {
            client,
            cookie_jar,
            device_id,
            pass_o,
            user_agent,
            auth_data: None,
            auth_updated: false,
        }
    }

    pub fn new_random() -> Self {
        let mut rng = rand::thread_rng();
        let device_id: String = (&mut rng)
            .sample_iter(&Alphanumeric)
            .take(16)
            .map(char::from)
            .collect();
        let pass_o = format!("{:016x}", rng.r#gen::<u64>());
        let ua_suffix: String = (&mut rng)
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        let user_agent = format!(
            "Android-15-Booter-1.0-Xiaomi-CN-{}|{}|{}-64",
            ua_suffix, device_id, pass_o
        );
        Self::new(device_id, pass_o, user_agent)
    }

    pub fn set_auth_data(&mut self, mut data: MijiaAuthData) {
        if data.device_id.is_empty() {
            data.device_id = self.device_id.clone();
        } else {
            self.device_id = data.device_id.clone();
        }
        if data.pass_o.is_empty() {
            data.pass_o = self.pass_o.clone();
        } else {
            self.pass_o = data.pass_o.clone();
        }
        if data.user_agent.is_empty() {
            data.user_agent = self.user_agent.clone();
        } else {
            self.user_agent = data.user_agent.clone();
        }
        self.auth_data = Some(data);
    }

    pub fn auth_was_updated(&self) -> bool {
        self.auth_updated
    }

    pub fn current_auth_data(&self) -> Option<&MijiaAuthData> {
        self.auth_data.as_ref()
    }

    pub fn decrypt_payload(ssecurity: &str, nonce: &str, payload: &str) -> Result<String, String> {
        let signed_nonce = get_signed_nonce(ssecurity, nonce);
        let decrypted = decrypt_rc4(&signed_nonce, payload);

        if let Ok(s) = String::from_utf8(decrypted.clone()) {
            Ok(s)
        } else {
            let mut gz = GzDecoder::new(decrypted.as_slice());
            let mut s = String::new();
            gz.read_to_string(&mut s)
                .map_err(|e| format!("Gzip error: {}", e))?;
            Ok(s)
        }
    }

    pub fn generate_enc_params(
        uri: &str,
        method: &str,
        signed_nonce: &str,
        nonce: &str,
        mut params: Vec<(String, String)>,
        ssecurity: &str,
    ) -> HashMap<String, String> {
        let rc4_hash = gen_enc_signature(uri, method, signed_nonce, &params);
        params.push(("rc4_hash__".into(), rc4_hash));

        let mut encrypted_params = Vec::new();
        for (k, v) in &params {
            encrypted_params.push((k.clone(), encrypt_rc4(signed_nonce, v)));
        }

        let signature = gen_enc_signature(uri, method, signed_nonce, &encrypted_params);

        let mut final_params: HashMap<String, String> = encrypted_params.into_iter().collect();
        final_params.insert("signature".into(), signature);
        final_params.insert("ssecurity".into(), ssecurity.into());
        final_params.insert("_nonce".into(), nonce.into());

        final_params
    }

    pub async fn qr_login_step1(&self) -> Result<(String, String), String> {
        // Return (login_url, lp_url) or Error
        let service_login_url =
            "https://account.xiaomi.com/pass/serviceLogin?_json=true&sid=mijia&_locale=zh_CN";

        let cookie_str = format!(
            "deviceId={};pass_o={};uLocale=zh_CN",
            self.device_id, self.pass_o
        );

        let res = self
            .client
            .get(service_login_url)
            .header(USER_AGENT, &self.user_agent)
            .header(COOKIE, cookie_str)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let text = res.text().await.map_err(|e| e.to_string())?;
        let clean_text = text.replace("&&&START&&&", "");
        let json: serde_json::Value =
            serde_json::from_str(&clean_text).map_err(|e| e.to_string())?;

        let location = json
            .get("location")
            .and_then(|v| v.as_str())
            .ok_or("No location in response")?;

        // Parse location URL query
        let parsed_url = reqwest::Url::parse(location).map_err(|e| e.to_string())?;
        let mut qs: HashMap<String, String> = parsed_url.query_pairs().into_owned().collect();

        qs.insert("theme".into(), "".into());
        qs.insert("bizDeviceType".into(), "".into());
        qs.insert("_hasLogo".into(), "false".into());
        qs.insert("_qrsize".into(), "240".into());
        qs.insert(
            "_dc".into(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
                .to_string(),
        );

        let login_url_req =
            reqwest::Url::parse_with_params("https://account.xiaomi.com/longPolling/loginUrl", &qs)
                .unwrap();

        let res2 = self
            .client
            .get(login_url_req)
            .header(USER_AGENT, &self.user_agent)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let text2 = res2.text().await.map_err(|e| e.to_string())?;
        let clean_text2 = text2.replace("&&&START&&&", "");
        let json2: serde_json::Value =
            serde_json::from_str(&clean_text2).map_err(|e| e.to_string())?;

        let qr_url = json2
            .get("loginUrl")
            .and_then(|v| v.as_str())
            .ok_or("No loginUrl")?
            .to_string();
        let lp_url = json2
            .get("lp")
            .and_then(|v| v.as_str())
            .ok_or("No lp")?
            .to_string();

        Ok((qr_url, lp_url))
    }

    pub async fn qr_login_step2(&mut self, lp_url: &str) -> Result<MijiaAuthData, String> {
        let lp_url = reqwest::Url::parse(lp_url).map_err(|e| format!("Invalid LP URL: {}", e))?;
        if lp_url.scheme() != "https" || lp_url.host_str() != Some("account.xiaomi.com") {
            return Err("Unexpected Mijia long-polling URL".into());
        }
        let res = self
            .client
            .get(lp_url)
            .header(USER_AGENT, &self.user_agent)
            .send()
            .await
            .map_err(|e| format!("LP request failed: {}", e))?;

        let text = res.text().await.map_err(|e| e.to_string())?;
        let clean_text = text.replace("&&&START&&&", "");
        let json: serde_json::Value =
            serde_json::from_str(&clean_text).map_err(|e| e.to_string())?;

        if json.get("code").and_then(|c| c.as_i64()).unwrap_or(-1) != 0 {
            let code = json.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
            let description = json
                .get("desc")
                .or_else(|| json.get("description"))
                .and_then(|value| value.as_str())
                .unwrap_or("login pending or timed out");
            return Err(format!("Mijia login failed ({}): {}", code, description));
        }

        let psecurity = json
            .get("psecurity")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let ssecurity = json
            .get("ssecurity")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let pass_token = json
            .get("passToken")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let user_id = json
            .get("userId")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let c_user_id = json
            .get("cUserId")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let location = json
            .get("location")
            .and_then(|v| v.as_str())
            .ok_or("No callback location")?;
        let callback_url =
            reqwest::Url::parse(location).map_err(|e| format!("Invalid callback URL: {}", e))?;
        if callback_url.scheme() != "https" {
            return Err("Mijia callback URL must use HTTPS".into());
        }

        let res2 = self
            .client
            .get(callback_url.clone())
            .header(USER_AGENT, &self.user_agent)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let mut service_token = String::new();
        for cookie_str in res2.headers().get_all(reqwest::header::SET_COOKIE) {
            if let Ok(c) = cookie_str.to_str() {
                if c.starts_with("serviceToken=") {
                    if let Some(token) = c
                        .strip_prefix("serviceToken=")
                        .and_then(|s| s.split(';').next())
                    {
                        service_token = token.to_string();
                    }
                }
            }
        }
        if service_token.is_empty() {
            let jar_cookies = self
                .cookie_jar
                .cookies(&callback_url)
                .and_then(|value| value.to_str().ok().map(str::to_owned))
                .unwrap_or_default();
            service_token = jar_cookies
                .split(';')
                .map(str::trim)
                .find_map(|cookie| {
                    cookie
                        .strip_prefix("serviceToken=")
                        .map(str::to_owned)
                })
                .unwrap_or_default();
        }

        if service_token.is_empty() {
            return Err("Failed to get serviceToken from callback".to_string());
        }

        let expire_time = SystemTime::now()
            .checked_add(std::time::Duration::from_secs(30 * 24 * 3600))
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let auth_data = MijiaAuthData {
            user_id,
            c_user_id,
            service_token,
            ssecurity,
            pass_token: Some(pass_token),
            psecurity: Some(psecurity),
            expire_time,
            device_id: self.device_id.clone(),
            pass_o: self.pass_o.clone(),
            user_agent: self.user_agent.clone(),
        };

        self.auth_data = Some(auth_data.clone());
        self.auth_updated = true;
        Ok(auth_data)
    }

    pub async fn refresh_auth_data(&mut self) -> Result<MijiaAuthData, String> {
        let auth = self.auth_data.clone().ok_or("Not authenticated")?;
        let pass_token = auth
            .pass_token
            .clone()
            .filter(|v| !v.is_empty())
            .ok_or("Mijia passToken is missing; please scan the QR code again")?;

        let cookie_str = format!(
            "deviceId={};pass_o={};passToken={};userId={};cUserId={};uLocale=zh_CN",
            self.device_id, self.pass_o, pass_token, auth.user_id, auth.c_user_id
        );
        let service_login_url =
            "https://account.xiaomi.com/pass/serviceLogin?_json=true&sid=mijia&_locale=zh_CN";
        let response = self
            .client
            .get(service_login_url)
            .header(USER_AGENT, &self.user_agent)
            .header(COOKIE, cookie_str)
            .send()
            .await
            .map_err(|e| format!("Mijia token refresh request failed: {}", e))?;

        let text = response.text().await.map_err(|e| e.to_string())?;
        let clean_text = text.replace("&&&START&&&", "");
        let json: serde_json::Value = serde_json::from_str(&clean_text)
            .map_err(|e| format!("Invalid Mijia refresh response: {}", e))?;
        if json.get("code").and_then(|v| v.as_i64()).unwrap_or(-1) != 0 {
            return Err("Mijia passToken expired; please scan the QR code again".into());
        }

        let location = json
            .get("location")
            .and_then(|v| v.as_str())
            .ok_or("Mijia refresh response did not contain a callback location")?;
        let callback_url = reqwest::Url::parse(location)
            .map_err(|e| format!("Invalid Mijia callback URL: {}", e))?;
        if callback_url.scheme() != "https" {
            return Err("Mijia callback URL must use HTTPS".into());
        }

        let callback_cookie = format!(
            "cUserId={};yetAnotherServiceToken={};serviceToken={};PassportDeviceId={};locale=zh_CN",
            auth.c_user_id, auth.service_token, auth.service_token, self.device_id
        );
        let callback_response = self
            .client
            .get(callback_url.clone())
            .header(USER_AGENT, &self.user_agent)
            .header(COOKIE, callback_cookie)
            .send()
            .await
            .map_err(|e| format!("Mijia refresh callback failed: {}", e))?;
        if !callback_response.status().is_success() {
            return Err(format!(
                "Mijia refresh callback returned HTTP {}",
                callback_response.status()
            ));
        }

        let jar_cookies = self
            .cookie_jar
            .cookies(&callback_url)
            .and_then(|value| value.to_str().ok().map(str::to_owned))
            .unwrap_or_default();
        let service_token = jar_cookies
            .split(';')
            .map(str::trim)
            .find_map(|cookie| cookie.strip_prefix("serviceToken=").map(str::to_owned))
            .filter(|value| !value.is_empty())
            .ok_or("Mijia refresh did not return a serviceToken")?;

        let mut refreshed = auth;
        refreshed.service_token = service_token;
        if let Some(ssecurity) = json.get("ssecurity").and_then(|v| v.as_str()) {
            refreshed.ssecurity = ssecurity.to_owned();
        }
        if let Some(c_user_id) = json.get("cUserId").and_then(|v| v.as_str()) {
            refreshed.c_user_id = c_user_id.to_owned();
        }
        refreshed.expire_time = SystemTime::now()
            .checked_add(std::time::Duration::from_secs(30 * 24 * 3600))
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        refreshed.device_id = self.device_id.clone();
        refreshed.pass_o = self.pass_o.clone();
        refreshed.user_agent = self.user_agent.clone();

        self.auth_data = Some(refreshed.clone());
        self.auth_updated = true;
        Ok(refreshed)
    }

    async fn request_once(
        &self,
        uri: &str,
        data: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let auth = self.auth_data.as_ref().ok_or("Not authenticated")?;
        let url = format!("https://api.mijia.tech/app{}", uri);

        let data_str = serde_json::to_string(data).map_err(|e| e.to_string())?;

        let nonce = gen_nonce();
        let signed_nonce = get_signed_nonce(&auth.ssecurity, &nonce);

        let params = vec![("data".into(), data_str)];

        let enc_params =
            Self::generate_enc_params(uri, "POST", &signed_nonce, &nonce, params, &auth.ssecurity);

        let cookie_str = format!(
            "cUserId={};yetAnotherServiceToken={};serviceToken={};timezone_id=Asia/Shanghai;locale=zh_CN;PassportDeviceId={};",
            auth.c_user_id, auth.service_token, auth.service_token, self.device_id
        );

        let res = self
            .client
            .post(&url)
            .header(USER_AGENT, &self.user_agent)
            .header(COOKIE, cookie_str)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header("miot-accept-encoding", "GZIP")
            .header("miot-encrypt-algorithm", "ENCRYPT-RC4")
            .form(&enc_params)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        let text = res.text().await.map_err(|e| e.to_string())?;

        let json: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => {
                let dec_str = Self::decrypt_payload(&auth.ssecurity, &nonce, &text)?;
                serde_json::from_str(&dec_str)
                    .map_err(|e| format!("Failed to parse decrypted json: {}", e))?
            }
        };

        if json.get("code").and_then(|c| c.as_i64()).unwrap_or(-1) != 0
            || json.get("result").is_none()
        {
            return Err(format!("API Error: {:?}", json));
        }

        Ok(json.get("result").unwrap().clone())
    }

    pub async fn request(
        &mut self,
        uri: &str,
        data: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let is_expired = self
            .auth_data
            .as_ref()
            .map(|auth| auth.expire_time <= now_millis)
            .unwrap_or(true);

        if is_expired {
            self.refresh_auth_data().await?;
        }

        match self.request_once(uri, data).await {
            Ok(result) => Ok(result),
            Err(first_error) => {
                self.refresh_auth_data().await.map_err(|refresh_error| {
                    format!("{}; token refresh failed: {}", first_error, refresh_error)
                })?;
                self.request_once(uri, data).await
            }
        }
    }

    pub async fn run_action(
        &mut self,
        did: &str,
        siid: i32,
        aiid: i32,
        value: Option<Vec<serde_json::Value>>,
    ) -> Result<serde_json::Value, String> {
        let mut param = serde_json::json!({
            "did": did,
            "siid": siid,
            "aiid": aiid
        });

        if let Some(v) = value {
            param
                .as_object_mut()
                .unwrap()
                .insert("in".into(), serde_json::Value::Array(v));
        }

        let data = serde_json::json!({
            "params": [param]
        });

        self.request("/miotspec/action", &data).await
    }

    pub async fn set_devices_prop(
        &mut self,
        did: &str,
        siid: i32,
        piid: i32,
        value: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let data = serde_json::json!({
            "params": [{
                "did": did,
                "siid": siid,
                "piid": piid,
                "value": value
            }]
        });

        self.request("/miotspec/prop/set", &data).await
    }
}

#[cfg(test)]
mod tests {
    use super::{MijiaAuthData, MijiaClient};

    #[test]
    fn legacy_credentials_gain_persisted_client_identity() {
        let auth: MijiaAuthData = serde_json::from_value(serde_json::json!({
            "user_id": "user",
            "c_user_id": "c-user",
            "service_token": "service-token",
            "ssecurity": "c2VjcmV0",
            "pass_token": "pass-token",
            "psecurity": "psecurity",
            "expire_time": 123
        }))
        .unwrap();

        let mut client = MijiaClient::new(
            "legacy-device".into(),
            "legacy-pass-o".into(),
            "legacy-user-agent".into(),
        );
        client.set_auth_data(auth);
        let normalized = client.current_auth_data().unwrap();

        assert_eq!(normalized.device_id, "legacy-device");
        assert_eq!(normalized.pass_o, "legacy-pass-o");
        assert_eq!(normalized.user_agent, "legacy-user-agent");
    }
}
