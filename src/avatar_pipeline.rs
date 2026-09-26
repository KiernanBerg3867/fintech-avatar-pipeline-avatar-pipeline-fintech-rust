use std::path::Path;

use serde::Serialize;
use thiserror::Error;

use crate::infrai_client::{ClientError, InfraiClient};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentEvent {
    Settled,
    Reversed,
    Disputed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AvatarDecision {
    Process,
    ManualReview,
    Block,
}

#[derive(Debug)]
pub struct AvatarRequest<'a> {
    pub user_id: &'a str,
    pub image_path: &'a Path,
    pub payment_event: PaymentEvent,
    pub risk_score: u8,
    pub operation_id: &'a str,
}

#[derive(Debug, Serialize)]
pub struct AuditNotification<'a> {
    pub operation_id: &'a str,
    pub user_id: &'a str,
    pub decision: AvatarDecision,
    pub processed_image_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("avatar request requires manual review")]
    ManualReview,
    #[error("avatar request was blocked by payment or risk policy")]
    Blocked,
    #[error("could not read avatar: {0}")]
    Read(#[from] std::io::Error),
    #[error(transparent)]
    Infrai(#[from] ClientError),
}

pub fn decide_avatar_action(event: PaymentEvent, risk_score: u8) -> AvatarDecision {
    match event {
        PaymentEvent::Reversed | PaymentEvent::Disputed => AvatarDecision::Block,
        PaymentEvent::Settled if risk_score >= 80 => AvatarDecision::Block,
        PaymentEvent::Settled if risk_score >= 50 => AvatarDecision::ManualReview,
        PaymentEvent::Settled => AvatarDecision::Process,
    }
}

pub async fn process_avatar<'a>(
    client: &InfraiClient,
    request: AvatarRequest<'a>,
) -> Result<AuditNotification<'a>, PipelineError> {
    let decision = decide_avatar_action(request.payment_event, request.risk_score);
    match decision {
        AvatarDecision::ManualReview => return Err(PipelineError::ManualReview),
        AvatarDecision::Block => return Err(PipelineError::Blocked),
        AvatarDecision::Process => {}
    }

    let bytes = tokio::fs::read(request.image_path).await?;
    let filename = request
        .image_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("avatar");
    let uploaded = client
        .upload_avatar(bytes, filename, request.operation_id)
        .await?;
    let cropped = client.smart_crop(&uploaded, request.operation_id).await?;
    let optimized = client.resize_avatar(&cropped, request.operation_id).await?;
    client
        .attach_avatar(request.user_id, &optimized, request.operation_id)
        .await?;

    Ok(AuditNotification {
        operation_id: request.operation_id,
        user_id: request.user_id,
        decision,
        processed_image_id: Some(optimized),
    })
}

#[cfg(test)]
mod tests {
    use super::{decide_avatar_action, AvatarDecision, PaymentEvent};

    #[test]
    fn payment_and_risk_policy_selects_the_action() {
        assert_eq!(
            decide_avatar_action(PaymentEvent::Settled, 18),
            AvatarDecision::Process
        );
        assert_eq!(
            decide_avatar_action(PaymentEvent::Settled, 65),
            AvatarDecision::ManualReview
        );
        assert_eq!(
            decide_avatar_action(PaymentEvent::Disputed, 3),
            AvatarDecision::Block
        );
        assert_eq!(
            decide_avatar_action(PaymentEvent::Reversed, 3),
            AvatarDecision::Block
        );
    }
}
