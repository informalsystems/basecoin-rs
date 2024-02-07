//! Contains methods specifically implemented for use with the Tower ABCI
//! interface, compatible with CometBFT version 0.37

use std::fmt::{Debug, Write};

use basecoin_modules::auth::ACCOUNT_PREFIX;
use basecoin_modules::types::{Error, IdentifiedModule};
use basecoin_store::context::{ProvableStore, Store};
use basecoin_store::types::{Height, Path};
use basecoin_store::utils::SharedRwExt;
use cosmrs::tx::{SignerInfo, SignerPublicKey};
use cosmrs::Tx;
use ibc_proto::google::protobuf::Any;
use prost::Message;
use serde_json::Value;
use tendermint::merkle::proof::{ProofOp, ProofOps};
use tendermint_proto::v0_37::abci::{
    response_process_proposal, Event as ProtoEvent, RequestApplySnapshotChunk, RequestBeginBlock,
    RequestCheckTx, RequestDeliverTx, RequestEcho, RequestEndBlock, RequestInfo, RequestInitChain,
    RequestLoadSnapshotChunk, RequestOfferSnapshot, RequestPrepareProposal, RequestProcessProposal,
    RequestQuery, ResponseApplySnapshotChunk, ResponseBeginBlock, ResponseCheckTx, ResponseCommit,
    ResponseDeliverTx, ResponseEcho, ResponseEndBlock, ResponseInfo, ResponseInitChain,
    ResponseListSnapshots, ResponseLoadSnapshotChunk, ResponseOfferSnapshot,
    ResponsePrepareProposal, ResponseProcessProposal, ResponseQuery,
};
use tracing::{debug, info};

use crate::utils::macros::ResponseFromErrorExt;
use crate::BaseCoinApp;

pub fn echo<S: Default + ProvableStore>(
    _app: &BaseCoinApp<S>,
    request: RequestEcho,
) -> ResponseEcho {
    ResponseEcho {
        message: request.message,
    }
}

pub fn info<S: Default + ProvableStore>(
    app: &BaseCoinApp<S>,
    request: RequestInfo,
) -> ResponseInfo {
    let (last_block_height, last_block_app_hash) = {
        let state = app.store.read_access();
        (state.current_height() as i64, state.root_hash())
    };
    debug!(
        "Got info request. Tendermint version: {}; Block version: {}; P2P version: {}, {:?}, {:?}",
        request.version,
        request.block_version,
        request.p2p_version,
        last_block_height,
        last_block_app_hash
    );
    ResponseInfo {
        data: "basecoin-rs".to_string(),
        version: "0.1.0".to_string(),
        app_version: 1,
        last_block_height,
        last_block_app_hash: last_block_app_hash.into(),
    }
}

pub fn init_chain<S: Default + ProvableStore>(
    app: &BaseCoinApp<S>,
    request: RequestInitChain,
) -> ResponseInitChain {
    debug!("Got init chain request.");

    // safety - we panic on errors to prevent chain creation with invalid genesis config
    let app_state: Value = serde_json::from_str(
        &String::from_utf8(request.app_state_bytes.clone().into()).expect("invalid genesis state"),
    )
    .expect("genesis state isn't valid JSON");
    let mut modules = app.modules.write_access();
    for IdentifiedModule { module, .. } in modules.iter_mut() {
        module.init(app_state.clone());
    }

    info!("App initialized");

    ResponseInitChain {
        consensus_params: request.consensus_params,
        validators: vec![], // use validator set proposed by tendermint (ie. in the genesis file)
        app_hash: app.store.write_access().root_hash().into(),
    }
}

pub fn query<S: Default + ProvableStore>(
    app: &BaseCoinApp<S>,
    request: RequestQuery,
) -> ResponseQuery {
    debug!("Got query request: {:?}", request);

    let path: Option<Path> = Some(request.path.into());
    let modules = app.modules.read_access();
    let height = Height::from(request.height as u64);
    for IdentifiedModule { id, module } in modules.iter() {
        match module.query(&request.data, path.as_ref(), height, request.prove) {
            // success - implies query was handled by this module, so return response
            Ok(result) => {
                let store = app.store.read_access();
                let proof_ops = if request.prove {
                    let proof = store.get_proof(height, &id.clone().into()).unwrap();
                    let mut buffer = Vec::new();
                    proof.encode(&mut buffer).unwrap(); // safety - cannot fail since buf is a vector

                    let mut ops = vec![];
                    if let Some(mut proofs) = result.proof {
                        ops.append(&mut proofs);
                    }
                    ops.push(ProofOp {
                        field_type: "".to_string(),
                        key: id.to_string().into_bytes(),
                        data: buffer,
                    });
                    Some(ProofOps { ops })
                } else {
                    None
                };

                return ResponseQuery {
                    code: 0,
                    log: "exists".to_string(),
                    key: request.data,
                    value: result.data.into(),
                    proof_ops: proof_ops.map(|proof_ops| proof_ops.into()),
                    height: store.current_height() as i64,
                    ..Default::default()
                };
            }
            // `Error::NotHandled` - implies query isn't known or was intercepted but not
            // responded to by this module, so try with next module
            Err(Error::NotHandled) => continue,
            // Other error - return immediately
            Err(e) => return ResponseQuery::from_error(1, format!("query error: {e:?}")),
        }
    }
    ResponseQuery::from_error(1, "query msg not handled")
}

pub fn check_tx<S: Default + ProvableStore>(
    _app: &BaseCoinApp<S>,
    _request: RequestCheckTx,
) -> ResponseCheckTx {
    Default::default()
}

pub fn deliver_tx<S: Default + Debug + ProvableStore>(
    app: &BaseCoinApp<S>,
    request: RequestDeliverTx,
) -> ResponseDeliverTx {
    debug!("Got deliverTx request: {request:?}");

    let tx: Tx = match request.tx.as_ref().try_into() {
        Ok(tx) => tx,
        Err(err) => {
            return ResponseDeliverTx::from_error(
                1,
                format!("failed to decode incoming tx bytes: {err}"),
            );
        }
    };

    // Extract `AccountId` of first signer
    let signer = {
        let pubkey = match tx.auth_info.signer_infos.first() {
            Some(&SignerInfo {
                public_key: Some(SignerPublicKey::Single(pubkey)),
                ..
            }) => pubkey,
            _ => return ResponseDeliverTx::from_error(2, "Empty signers"),
        };
        if let Ok(signer) = pubkey.account_id(ACCOUNT_PREFIX) {
            signer
        } else {
            return ResponseDeliverTx::from_error(2, "Invalid signer");
        }
    };

    if tx.body.messages.is_empty() {
        return ResponseDeliverTx::from_error(2, "Empty Tx");
    }

    let mut events = vec![];
    for message in tx.body.messages {
        let message = Any {
            type_url: message.type_url,
            value: message.value,
        };

        // try to deliver message to every module
        match app.deliver_msg(message, &signer) {
            // success - append events and continue with next message
            Ok(msg_events) => {
                let mut proto_events: Vec<ProtoEvent> =
                    msg_events.into_iter().map(|event| event.into()).collect();

                events.append(&mut proto_events);
            }
            // return on first error -
            // either an error that occurred during execution of this message OR no module
            // could handle this message
            Err(e) => {
                // reset changes from other messages in this tx
                let mut modules = app.modules.write_access();
                for IdentifiedModule { module, .. } in modules.iter_mut() {
                    module.store_mut().reset();
                }
                app.store.write_access().reset();
                return ResponseDeliverTx::from_error(2, format!("deliver failed with error: {e}"));
            }
        }
    }

    ResponseDeliverTx {
        log: "success".to_owned(),
        events,
        ..ResponseDeliverTx::default()
    }
}

pub fn commit<S: Default + ProvableStore>(app: &BaseCoinApp<S>) -> ResponseCommit {
    let mut modules = app.modules.write_access();
    for IdentifiedModule { id, module } in modules.iter_mut() {
        module
            .store_mut()
            .commit()
            .expect("failed to commit to state");
        let mut state = app.store.write_access();
        state
            .set(id.clone().into(), module.store().root_hash())
            .expect("failed to update sub-store commitment");
    }

    let mut state = app.store.write_access();
    let data = state.commit().expect("failed to commit to state");
    info!(
        "Committed height {} with hash({})",
        state.current_height() - 1,
        data.iter().fold(String::new(), |mut acc, b| {
            // write!-ing into a String can never fail
            let _ = write!(acc, "{b:02X}");
            acc
        })
    );
    ResponseCommit {
        data: data.into(),
        retain_height: 0,
    }
}

pub fn begin_block<S: Default + ProvableStore>(
    app: &BaseCoinApp<S>,
    request: RequestBeginBlock,
) -> ResponseBeginBlock {
    debug!("Got begin block request.");

    let mut modules = app.modules.write_access();
    let mut events = vec![];
    let header = request.header.unwrap().try_into().unwrap();
    for IdentifiedModule { module, .. } in modules.iter_mut() {
        let tm_event = module.begin_block(&header);

        let proto_events: Vec<ProtoEvent> =
            tm_event.into_iter().map(|event| event.into()).collect();

        events.extend(proto_events);
    }

    ResponseBeginBlock { events }
}

pub fn end_block<S: Default + ProvableStore>(
    _app: &BaseCoinApp<S>,
    _request: RequestEndBlock,
) -> ResponseEndBlock {
    Default::default()
}

pub fn list_snapshots<S: Default + ProvableStore>(_app: &BaseCoinApp<S>) -> ResponseListSnapshots {
    Default::default()
}

pub fn offer_snapshot<S: Default + ProvableStore>(
    _app: &BaseCoinApp<S>,
    _request: RequestOfferSnapshot,
) -> ResponseOfferSnapshot {
    Default::default()
}

pub fn load_snapshot_chunk<S: Default + ProvableStore>(
    _app: &BaseCoinApp<S>,
    _request: RequestLoadSnapshotChunk,
) -> ResponseLoadSnapshotChunk {
    Default::default()
}

pub fn apply_snapshot_chunk<S: Default + ProvableStore>(
    _app: &BaseCoinApp<S>,
    _request: RequestApplySnapshotChunk,
) -> ResponseApplySnapshotChunk {
    Default::default()
}

pub fn prepare_proposal<S: Default + ProvableStore>(
    _app: &BaseCoinApp<S>,
    request: RequestPrepareProposal,
) -> ResponsePrepareProposal {
    let RequestPrepareProposal {
        mut txs,
        max_tx_bytes,
        ..
    } = request;
    let max_tx_bytes: usize = max_tx_bytes.try_into().unwrap_or(0);
    let mut total_tx_bytes: usize = txs
        .iter()
        .map(|tx| tx.len())
        .fold(0, |acc, len| acc.saturating_add(len));
    while total_tx_bytes > max_tx_bytes {
        if let Some(tx) = txs.pop() {
            total_tx_bytes = total_tx_bytes.saturating_sub(tx.len());
        } else {
            break;
        }
    }
    ResponsePrepareProposal { txs }
}

pub fn process_proposal<S: Default + ProvableStore>(
    _app: &BaseCoinApp<S>,
    _request: RequestProcessProposal,
) -> ResponseProcessProposal {
    ResponseProcessProposal {
        status: response_process_proposal::ProposalStatus::Accept as i32,
    }
}
