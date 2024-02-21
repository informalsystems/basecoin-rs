//! Definition of domain `Proposal`.

use ibc_proto::cosmos::gov::v1beta1::{Proposal as RawProposal, ProposalStatus, TallyResult};
use ibc_proto::google::protobuf::{Any, Timestamp};
use ibc_proto::Protobuf;

use super::error::Error;
use crate::bank::Coin;

pub(crate) const TYPE_URL: &str = "/cosmos.gov.v1beta1.Proposal";

#[derive(Clone, Debug)]
pub struct Proposal {
    pub proposal_id: u64,
    pub content: Any,
    pub status: ProposalStatus,
    pub final_tally_result: Option<TallyResult>,
    pub submit_time: Option<Timestamp>,
    pub deposit_end_time: Option<Timestamp>,
    pub total_deposit: Coin,
    pub voting_start_time: Option<Timestamp>,
    pub voting_end_time: Option<Timestamp>,
}

impl Protobuf<RawProposal> for Proposal {}

impl TryFrom<RawProposal> for Proposal {
    type Error = Error;

    fn try_from(raw: RawProposal) -> Result<Self, Self::Error> {
        let total_deposit: Coin = raw.total_deposit[0].clone().try_into().unwrap();

        let status = match raw.status {
            0 => ProposalStatus::Unspecified,
            1 => ProposalStatus::DepositPeriod,
            2 => ProposalStatus::VotingPeriod,
            3 => ProposalStatus::Passed,
            4 => ProposalStatus::Rejected,
            5 => ProposalStatus::Failed,
            _ => {
                return Err(Error::InvalidProposal {
                    reason: "invalid status".into(),
                })
            }
        };
        Ok(Proposal {
            proposal_id: raw.proposal_id,
            content: raw.content.unwrap(),
            status,
            final_tally_result: raw.final_tally_result,
            submit_time: None,
            deposit_end_time: None,
            total_deposit,
            voting_start_time: None,
            voting_end_time: None,
        })
    }
}

impl From<Proposal> for RawProposal {
    fn from(value: Proposal) -> Self {
        let status = match value.status {
            ProposalStatus::Unspecified => 0,
            ProposalStatus::DepositPeriod => 1,
            ProposalStatus::VotingPeriod => 2,
            ProposalStatus::Passed => 3,
            ProposalStatus::Rejected => 4,
            ProposalStatus::Failed => 5,
        };

        Self {
            proposal_id: value.proposal_id,
            content: Some(value.content),
            status,
            final_tally_result: value.final_tally_result,
            submit_time: value.submit_time,
            deposit_end_time: value.deposit_end_time,
            total_deposit: vec![value.total_deposit.into()],
            voting_start_time: value.voting_start_time,
            voting_end_time: value.voting_end_time,
        }
    }
}

impl Protobuf<Any> for Proposal {}

impl TryFrom<Any> for Proposal {
    type Error = Error;

    fn try_from(raw: Any) -> Result<Self, Self::Error> {
        match raw.type_url.as_str() {
            TYPE_URL => Protobuf::<RawProposal>::decode_vec(&raw.value).map_err(|e| {
                Error::InvalidProposal {
                    reason: e.to_string(),
                }
            }),
            _ => Err(Error::InvalidProposal {
                reason: raw.type_url,
            }),
        }
    }
}

impl From<Proposal> for Any {
    fn from(value: Proposal) -> Self {
        Self {
            type_url: TYPE_URL.to_string(),
            value: Protobuf::<RawProposal>::encode_vec(value),
        }
    }
}
