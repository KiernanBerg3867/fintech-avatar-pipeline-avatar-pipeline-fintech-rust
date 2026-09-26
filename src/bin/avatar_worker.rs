use std::{env, path::Path};

use fintech_avatar_pipeline::{
    avatar_pipeline::{process_avatar, AvatarRequest, PaymentEvent},
    infrai_client::InfraiClient,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let user_id = required(&mut args, "user_id")?;
    let image_path = required(&mut args, "image_path")?;
    let operation_id = required(&mut args, "operation_id")?;
    let risk_score = required(&mut args, "risk_score")?.parse::<u8>()?;
    let payment_event = match required(&mut args, "payment_event")?.as_str() {
        "settled" => PaymentEvent::Settled,
        "reversed" => PaymentEvent::Reversed,
        "disputed" => PaymentEvent::Disputed,
        _ => return Err("payment_event must be settled, reversed, or disputed".into()),
    };

    let client = InfraiClient::from_env()?;
    let notice = process_avatar(
        &client,
        AvatarRequest {
            user_id: &user_id,
            image_path: Path::new(&image_path),
            payment_event,
            risk_score,
            operation_id: &operation_id,
        },
    )
    .await?;

    println!("{}", serde_json::to_string(&notice)?);
    Ok(())
}

fn required(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("missing argument: {name}"))
}
