use super::response::ResponseFromErrorExt;
use super::Application;
use crate::modules::{Error, ErrorDetail};
use crate::store::{Height, Path, ProvableStore, Store};

use std::convert::TryInto;

use cosmrs::Tx;
use prost::Message;
use serde_json::Value;
use tendermint_abci::Application as AbciApplication;
use tendermint_proto::abci::{
    RequestBeginBlock, RequestDeliverTx, RequestInfo, RequestInitChain, RequestQuery,
    ResponseBeginBlock, ResponseCommit, ResponseDeliverTx, ResponseInfo, ResponseInitChain,
    ResponseQuery,
};
use tendermint_proto::crypto::ProofOp;
use tendermint_proto::crypto::ProofOps;
use tracing::{debug, info};

impl<S: Default + ProvableStore + 'static> AbciApplication for Application<S> {
    fn info(&self, request: RequestInfo) -> ResponseInfo {
        let (last_block_height, last_block_app_hash) = {
            let state = self.store.read().unwrap();
            (state.current_height() as i64, state.root_hash())
        };
        debug!(
            "Got info request. Tendermint version: {}; Block version: {}; P2P version: {}, {:?}, {:?}",
            request.version, request.block_version, request.p2p_version, last_block_height, last_block_app_hash
        );
        ResponseInfo {
            data: "basecoin-rs".to_string(),
            version: "0.1.0".to_string(),
            app_version: 1,
            last_block_height,
            last_block_app_hash,
        }
    }

    fn init_chain(&self, request: RequestInitChain) -> ResponseInitChain {
        debug!("Got init chain request.");

        // safety - we panic on errors to prevent chain creation with invalid genesis config
        let app_state: Value = serde_json::from_str(
            &String::from_utf8(request.app_state_bytes.clone()).expect("invalid genesis state"),
        )
        .expect("genesis state isn't valid JSON");
        let mut modules = self.modules.write().unwrap();
        for (_, m) in modules.iter_mut() {
            m.init(app_state.clone());
        }

        info!("App initialized");

        ResponseInitChain {
            consensus_params: request.consensus_params,
            validators: vec![], // use validator set proposed by tendermint (ie. in the genesis file)
            app_hash: self.store.write().unwrap().root_hash(),
        }
    }

    fn query(&self, request: RequestQuery) -> ResponseQuery {
        debug!("Got query request: {:?}", request);

        let path: Option<Path> = request.path.try_into().ok();
        let modules = self.modules.read().unwrap();
        let height = Height::from(request.height as u64);
        for (_, m) in modules.iter() {
            match m.query(&request.data, path.as_ref(), height, request.prove) {
                // success - implies query was handled by this module, so return response
                Ok(result) => {
                    let store = self.store.read().unwrap();
                    let proof_ops = if request.prove {
                        let proof = store
                            .get_proof(height, &"ibc".to_owned().try_into().unwrap())
                            .unwrap();
                        let mut buffer = Vec::new();
                        proof.encode(&mut buffer).unwrap(); // safety - cannot fail since buf is a vector

                        let mut ops = vec![];
                        if let Some(mut proofs) = result.proof {
                            ops.append(&mut proofs);
                        }
                        ops.push(ProofOp {
                            r#type: "".to_string(),
                            // FIXME(hu55a1n1)
                            key: "ibc".to_string().into_bytes(),
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
                        value: result.data,
                        proof_ops,
                        height: store.current_height() as i64,
                        ..Default::default()
                    };
                }
                // `Error::not_handled()` - implies query isn't known or was intercepted but not
                // responded to by this module, so try with next module
                Err(Error(ErrorDetail::NotHandled(_), _)) => continue,
                // Other error - return immediately
                Err(e) => return ResponseQuery::from_error(1, format!("query error: {:?}", e)),
            }
        }
        ResponseQuery::from_error(1, "query msg not handled")
    }

    fn deliver_tx(&self, request: RequestDeliverTx) -> ResponseDeliverTx {
        debug!("Got deliverTx request: {:?}", request);

        let tx: Tx = match request.tx.as_slice().try_into() {
            Ok(tx) => tx,
            Err(err) => {
                return ResponseDeliverTx::from_error(
                    1,
                    format!("failed to decode incoming tx bytes: {}", err),
                );
            }
        };

        if tx.body.messages.is_empty() {
            return ResponseDeliverTx::from_error(2, "Empty Tx");
        }

        let mut events = vec![];
        for message in tx.body.messages {
            // try to deliver message to every module
            match self.deliver_msg(message.clone()) {
                // success - append events and continue with next message
                Ok(mut msg_events) => {
                    events.append(&mut msg_events);
                }
                // return on first error -
                // either an error that occurred during execution of this message OR no module
                // could handle this message
                Err(e) => {
                    // reset changes from other messages in this tx
                    let mut modules = self.modules.write().unwrap();
                    for (_, m) in modules.iter_mut() {
                        m.store_mut().reset();
                    }
                    self.store.write().unwrap().reset();
                    return ResponseDeliverTx::from_error(
                        2,
                        format!("deliver failed with error: {}", e),
                    );
                }
            }
        }

        ResponseDeliverTx {
            log: "success".to_owned(),
            events,
            ..ResponseDeliverTx::default()
        }
    }

    fn commit(&self) -> ResponseCommit {
        let mut modules = self.modules.write().unwrap();
        for (p, m) in modules.iter_mut() {
            m.store_mut().commit().expect("failed to commit to state");
            let mut state = self.store.write().unwrap();
            state
                .set(p.clone().into(), m.store().root_hash())
                .expect("failed to update sub-store commitment");
        }

        let mut state = self.store.write().unwrap();
        let data = state.commit().expect("failed to commit to state");
        info!(
            "Committed height {} with hash({})",
            state.current_height() - 1,
            data.iter()
                .map(|b| format!("{:02X}", b))
                .collect::<String>()
        );
        ResponseCommit {
            data,
            retain_height: 0,
        }
    }

    fn begin_block(&self, request: RequestBeginBlock) -> ResponseBeginBlock {
        debug!("Got begin block request.");

        let mut modules = self.modules.write().unwrap();
        let mut events = vec![];
        let header = request.header.unwrap().try_into().unwrap();
        for (_, m) in modules.iter_mut() {
            events.extend(m.begin_block(&header));
        }

        ResponseBeginBlock { events }
    }
}
