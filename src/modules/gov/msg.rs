use displaydoc::Display;

use ibc_proto::cosmos::gov::v1beta1::MsgSubmitProposal as RawMsgSubmitProposal;
use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Protobuf;

use crate::helper::error::Error;
use crate::modules::bank::util::Coin;

pub(crate) const TYPE_URL: &str = "/cosmos.gov.v1beta1.MsgSubmitProposal";

#[derive(Clone, Debug, Display)]
pub struct MsgSubmitProposal {
    pub content: Any,
    pub initial_deposit: Coin,
    pub proposer: String,
}

impl Protobuf<RawMsgSubmitProposal> for MsgSubmitProposal {}

impl TryFrom<RawMsgSubmitProposal> for MsgSubmitProposal {
    type Error = Error;

    fn try_from(raw: RawMsgSubmitProposal) -> Result<Self, Self::Error> {
        let coin: Coin = raw.initial_deposit[0].clone().try_into()?;

        Ok(Self {
            content: raw.content.unwrap(),
            initial_deposit: coin,
            proposer: raw.proposer,
        })
    }
}

impl From<MsgSubmitProposal> for RawMsgSubmitProposal {
    fn from(value: MsgSubmitProposal) -> Self {
        Self {
            content: Some(value.content),
            initial_deposit: vec![value.initial_deposit.into()],
            proposer: value.proposer,
        }
    }
}

impl TryFrom<Any> for MsgSubmitProposal {
    type Error = Error;

    fn try_from(raw: Any) -> Result<Self, Self::Error> {
        match raw.type_url.as_str() {
            TYPE_URL => MsgSubmitProposal::decode_vec(&raw.value).map_err(|e| Error::Other {
                reason: e.to_string(),
            }),
            _ => Err(Error::Other {
                reason: raw.type_url,
            }),
        }
    }
}
