use displaydoc::Display;

use ibc_proto::cosmos::gov::v1beta1::MsgSubmitProposal as RawMsgSubmitProposal;
use ibc_proto::cosmos::gov::v1beta1::ProposalStatus;
use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Protobuf;

use crate::modules::bank::util::Coin;
use crate::types::error::Error;

use super::proposal::Proposal;

pub(crate) const TYPE_URL: &str = "/cosmos.gov.v1beta1.MsgSubmitProposal";

#[derive(Clone, Debug, Display)]
pub struct MsgSubmitProposal {
    pub content: Any,
    pub initial_deposit: Coin,
    pub proposer: String,
}

impl MsgSubmitProposal {
    pub fn proposal(&self, proposal_counter: u64) -> Proposal {
        Proposal {
            proposal_id: proposal_counter,
            content: self.content.clone(),
            status: ProposalStatus::VotingPeriod,
            final_tally_result: None,
            submit_time: None,
            deposit_end_time: None,
            total_deposit: self.initial_deposit.clone(),
            voting_start_time: None,
            voting_end_time: None,
        }
    }
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
            TYPE_URL => MsgSubmitProposal::decode_vec(&raw.value).map_err(|e| Error::Custom {
                reason: e.to_string(),
            }),
            _ => Err(Error::Custom {
                reason: raw.type_url,
            }),
        }
    }
}
