use cosmos_sdk::bank::MsgSend;
use cosmos_sdk::Tx;
use prost::Message;
use prost_types::Any;
use std::convert::TryFrom;
use std::str::FromStr;
use tracing::debug;

/// Validate the given transaction data, decoding it as a `MsgSend` structure
/// if it is valid.
pub fn validate_tx(tx: Tx) -> std::result::Result<MsgSend, (u32, String)> {
    if tx.body.messages.is_empty() {
        debug!("Got empty tx body");
        return Err((2, "no messages in incoming transaction".to_string()));
    }
    let msg_any = Any::from(tx.body.messages[0].clone());
    debug!("Got Protobuf Any message: {:?}", msg_any);
    if msg_any.type_url != "/cosmos.bank.v1beta1.MsgSend" {
        return Err((
            3,
            format!(
                "expected message type \"/cosmos.bank.v1beta1.MsgSend\", but got \"{}\"",
                msg_any.type_url
            ),
        ));
    }
    let proto_msg_send: cosmos_sdk_proto::cosmos::bank::v1beta1::MsgSend =
        match Message::decode(msg_any.value.as_ref()) {
            Ok(m) => m,
            Err(e) => return Err((4, e.to_string())),
        };
    debug!("Protobuf MsgSend: {:?}", proto_msg_send);
    let msg = match MsgSend::try_from(proto_msg_send) {
        Ok(m) => m,
        Err(e) => {
            debug!(
                "Failed to decode a bank send tx from {:?}\n\n{:?}",
                tx.body.messages[0], e
            );
            return Err((5, e.to_string()));
        }
    };
    if let Err(e) = u64::from_str(msg.amount[0].amount.to_string().as_str()) {
        return Err((6, format!("failed to decode amount: {}", e.to_string())));
    }
    Ok(msg)
}
