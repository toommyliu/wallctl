use anyhow::Error;
use chrono::{DateTime, FixedOffset};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct ApiEnvelope<T> {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
}

impl<T> ApiEnvelope<T> {
    pub fn success(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn failure(error: &Error) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(ApiError {
                message: format!("{error:#}"),
            }),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ApiError {
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ApiStatus {
    pub active_collection: Option<String>,
    pub expected_profile: Option<String>,
    pub last_applied_profile: Option<String>,
    pub last_applied_at: Option<DateTime<FixedOffset>>,
    pub live_matches_profile: Option<bool>,
    pub issues: Vec<String>,
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;

    use super::ApiEnvelope;

    #[test]
    fn failure_envelope_contains_full_error_chain() {
        let error = anyhow!("inner").context("outer");
        let envelope: ApiEnvelope<()> = ApiEnvelope::failure(&error);

        assert!(!envelope.ok);
        assert_eq!(envelope.error.unwrap().message, "outer: inner");
    }
}
