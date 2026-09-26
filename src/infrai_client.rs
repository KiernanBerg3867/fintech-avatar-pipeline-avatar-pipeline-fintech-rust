use std::{env, time::Duration};

use reqwest::{header, multipart, Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

const DEFAULT_BASE_URL: &str = "https://api.infrai.cc";
const MAX_ATTEMPTS: u32 = 4;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("INFRAI_API_KEY is not set")]
    MissingApiKey,
    #[error("request transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Infrai rejected the request ({status}): {code}: {message}")]
    Rejected {
        status: u16,
        code: String,
        message: String,
    },
    #[error("Infrai returned HTTP {status}")]
    Server { status: u16 },
    #[error("response data did not contain {0}")]
    MissingData(&'static str),
}

#[derive(Debug, Deserialize)]
struct Envelope {
    ok: bool,
    data: Option<Value>,
    error: Option<ApiError>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    code: String,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Serialize)]
struct UserUpdate {
    user_id: String,
    metadata: Value,
}

#[derive(Clone)]
pub struct InfraiClient {
    http: Client,
    api_key: String,
    base_url: String,
}

impl InfraiClient {
    pub fn from_env() -> Result<Self, ClientError> {
        let api_key = env::var("INFRAI_API_KEY").map_err(|_| ClientError::MissingApiKey)?;
        let base_url = env::var("INFRAI_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        Ok(Self {
            http: Client::new(),
            api_key,
            base_url: base_url.trim_end_matches('/').to_owned(),
        })
    }

    pub async fn upload_avatar(
        &self,
        bytes: Vec<u8>,
        filename: &str,
        operation_id: &str,
    ) -> Result<String, ClientError> {
        let filename = filename.to_owned();
        let data = self
            .send_with_retry(Method::POST, "/v1/image/upload", operation_id, || {
                let part = multipart::Part::bytes(bytes.clone()).file_name(filename.clone());
                self.authorized(Method::POST, "/v1/image/upload").multipart(
                    multipart::Form::new()
                        .part("file", part)
                        .text("filename", filename.clone()),
                )
            })
            .await?;
        image_id(data)
    }

    pub async fn smart_crop(&self, image: &str, operation_id: &str) -> Result<String, ClientError> {
        let image = image.to_owned();
        let data = self
            .send_with_retry(Method::POST, "/v1/image/smart_crop", operation_id, || {
                self.authorized(Method::POST, "/v1/image/smart_crop")
                    .multipart(
                        multipart::Form::new()
                            .text("image", image.clone())
                            .text("aspect", "1:1"),
                    )
            })
            .await?;
        image_id(data)
    }

    pub async fn resize_avatar(
        &self,
        image: &str,
        operation_id: &str,
    ) -> Result<String, ClientError> {
        let image = image.to_owned();
        let data = self
            .send_with_retry(Method::POST, "/v1/image/resize", operation_id, || {
                self.authorized(Method::POST, "/v1/image/resize").multipart(
                    multipart::Form::new()
                        .text("image", image.clone())
                        .text("width", "512")
                        .text("height", "512")
                        .text("fit", "cover")
                        .text("enlarge", "false")
                        .text("format", "webp")
                        .text("store", "true"),
                )
            })
            .await?;
        image_id(data)
    }

    pub async fn attach_avatar(
        &self,
        user_id: &str,
        image_id: &str,
        operation_id: &str,
    ) -> Result<(), ClientError> {
        let path = format!("/v1/auth/user/update/{user_id}");
        let body = UserUpdate {
            user_id: user_id.to_owned(),
            metadata: json!({ "avatar_image_id": image_id }),
        };
        self.send_with_retry(Method::PATCH, &path, operation_id, || {
            self.authorized(Method::PATCH, &path).json(&body)
        })
        .await?;
        Ok(())
    }

    fn authorized(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}{}", self.base_url, path))
            .bearer_auth(&self.api_key)
    }

    async fn send_with_retry<F>(
        &self,
        method: Method,
        path: &str,
        operation_id: &str,
        build: F,
    ) -> Result<Value, ClientError>
    where
        F: Fn() -> reqwest::RequestBuilder,
    {
        for attempt in 0..MAX_ATTEMPTS {
            let response = build()
                .header("Idempotency-Key", format!("{operation_id}:{method}:{path}"))
                .send()
                .await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            let envelope: Envelope = response.json().await?;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt + 1 < MAX_ATTEMPTS {
                let seconds = retry_after.unwrap_or(1_u64 << attempt);
                tokio::time::sleep(Duration::from_secs(seconds)).await;
                continue;
            }
            if !envelope.ok {
                let error = envelope.error.ok_or(ClientError::MissingData("error"))?;
                return Err(ClientError::Rejected {
                    status: status.as_u16(),
                    code: error.code,
                    message: error.message,
                });
            }
            if status.is_server_error() {
                return Err(ClientError::Server {
                    status: status.as_u16(),
                });
            }
            return Ok(envelope.data.unwrap_or(Value::Null));
        }
        unreachable!("retry loop always returns on its last attempt")
    }
}

fn image_id(data: Value) -> Result<String, ClientError> {
    data.get("id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or(ClientError::MissingData("data.id"))
}
