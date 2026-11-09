use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct RegistryClient {
    base_url: String,
    http: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    #[serde(rename = "fingerprintHash")]
    pub fingerprint_hash: String,
    #[serde(rename = "peerId")]
    pub peer_id: Option<String>,
    pub multiaddrs: Vec<String>,
    pub os: Option<String>,
    #[serde(rename = "lastSeenAt")]
    pub last_seen_at: Option<String>,
    pub online: bool,
    #[serde(rename = "isSelf")]
    pub is_self: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResponse {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "userId")]
    pub user_id: String,
    #[serde(rename = "deviceId")]
    pub device_id: String,
    pub email: String,
}

impl RegistryClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    pub async fn request_otp(&self, email: &str) -> Result<()> {
        let url = format!("{}/auth/request-otp", self.base_url);
        let res = self
            .http
            .post(&url)
            .json(&serde_json::json!({ "email": email }))
            .send()
            .await
            .with_context(|| format!("request otp ({url})"))?;
        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(anyhow!("request otp failed ({status}): {body}"));
        }
        Ok(())
    }

    pub async fn verify_otp(
        &self,
        email: &str,
        otp: &str,
        device_name: &str,
        fingerprint_hash: &str,
        peer_id: &str,
        multiaddrs: &[String],
        os: &str,
    ) -> Result<VerifyResponse> {
        let res = self
            .http
            .post(format!("{}/auth/verify-otp", self.base_url))
            .json(&serde_json::json!({
                "email": email,
                "otp": otp,
                "device": {
                    "name": device_name,
                    "fingerprintHash": fingerprint_hash,
                    "peerId": peer_id,
                    "multiaddrs": multiaddrs,
                    "os": os,
                }
            }))
            .send()
            .await
            .context("verify otp")?;
        if !res.status().is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(anyhow!("verify otp failed: {body}"));
        }
        Ok(res.json().await?)
    }

    pub async fn list_devices(&self, token: &str) -> Result<Vec<DeviceInfo>> {
        #[derive(Deserialize)]
        struct Resp {
            devices: Vec<DeviceInfo>,
        }
        let res = self
            .http
            .get(format!("{}/devices", self.base_url))
            .bearer_auth(token)
            .send()
            .await
            .context("list devices")?;
        if !res.status().is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(anyhow!("list devices failed: {body}"));
        }
        let parsed: Resp = res.json().await?;
        Ok(parsed.devices)
    }

    pub async fn update_me(
        &self,
        token: &str,
        peer_id: &str,
        multiaddrs: &[String],
        name: Option<&str>,
    ) -> Result<()> {
        let mut body = serde_json::json!({
            "peerId": peer_id,
            "multiaddrs": multiaddrs,
        });
        if let Some(n) = name {
            body["name"] = serde_json::json!(n);
        }
        let res = self
            .http
            .put(format!("{}/devices/me", self.base_url))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .context("update device")?;
        if !res.status().is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(anyhow!("update device failed: {body}"));
        }
        Ok(())
    }

    pub async fn heartbeat(&self, token: &str) -> Result<()> {
        let res = self
            .http
            .post(format!("{}/devices/me/heartbeat", self.base_url))
            .bearer_auth(token)
            .send()
            .await
            .context("heartbeat")?;
        if !res.status().is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(anyhow!("heartbeat failed: {body}"));
        }
        Ok(())
    }

    pub async fn unlink_device(&self, token: &str, device_id: &str) -> Result<()> {
        let res = self
            .http
            .delete(format!("{}/devices/{device_id}", self.base_url))
            .bearer_auth(token)
            .send()
            .await
            .context("unlink device")?;
        if !res.status().is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(anyhow!("unlink failed: {body}"));
        }
        Ok(())
    }

    pub async fn relay_multiaddr(&self) -> Result<Option<String>> {
        #[derive(Deserialize)]
        struct Resp {
            #[serde(rename = "relayMultiaddr")]
            relay_multiaddr: Option<String>,
        }
        let res = self
            .http
            .get(format!("{}/config", self.base_url))
            .send()
            .await?;
        if !res.status().is_success() {
            return Ok(None);
        }
        let parsed: Resp = res.json().await?;
        Ok(parsed.relay_multiaddr)
    }
}
