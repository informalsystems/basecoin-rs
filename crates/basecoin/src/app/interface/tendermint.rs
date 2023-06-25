use prost::Message;
use serde_json::Value;
use std::convert::TryInto;
use tracing::{debug, info};

use cosmrs::tx::SignerInfo;
use cosmrs::tx::SignerPublicKey;
use cosmrs::Tx;

use ibc_proto::google::protobuf::Any;

use tendermint_abci::Application;
use tendermint_proto::abci::RequestBeginBlock;
use tendermint_proto::abci::RequestDeliverTx;
use tendermint_proto::abci::RequestInfo;
use tendermint_proto::abci::RequestInitChain;
use tendermint_proto::abci::RequestQuery;
use tendermint_proto::abci::ResponseBeginBlock;
use tendermint_proto::abci::ResponseCommit;
use tendermint_proto::abci::ResponseDeliverTx;
use tendermint_proto::abci::ResponseInfo;
use tendermint_proto::abci::ResponseInitChain;
use tendermint_proto::abci::ResponseQuery;
use tendermint_proto::crypto::ProofOp;
use tendermint_proto::crypto::ProofOps;

use crate::app::BaseCoinApp;
use cosmos_sdk_rs_helper::macros::ResponseFromErrorExt;
use cosmos_sdk_rs_helper::{Height, Path};
use cosmos_sdk_rs_auth::account::ACCOUNT_PREFIX;
use cosmos_sdk_rs_module_api::types::IdentifiedModule;
use cosmos_sdk_rs_store::{ProvableStore, Store};

impl<S: Default + ProvableStore + 'static> Application for BaseCoinApp<S> {
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
            last_block_app_hash: last_block_app_hash.into(),
        }
    }

    fn init_chain(&self, request: RequestInitChain) -> ResponseInitChain {
        debug!("Got init chain request.");

        // safety - we panic on errors to prevent chain creation with invalid genesis config
        let app_state: Value = serde_json::from_str(
            &String::from_utf8(request.app_state_bytes.clone().into())
                .expect("invalid genesis state"),
        )
        .expect("genesis state isn't valid JSON");
        let mut modules = self.modules.write().unwrap();
        for IdentifiedModule { module, .. } in modules.iter_mut() {
            module.init(app_state.clone());
        }

        info!("App initialized");

        ResponseInitChain {
            consensus_params: request.consensus_params,
            validators: vec![], // use validator set proposed by tendermint (ie. in the genesis file)
            app_hash: self.store.write().unwrap().root_hash().into(),
        }
    }

    fn query(&self, request: RequestQuery) -> ResponseQuery {
        debug!("Got query request: {:?}", request);

        let path: Option<Path> = request.path.try_into().ok();
        let modules = self.modules.read().unwrap();
        let height = Height::from(request.height as u64);
        for IdentifiedModule { id, module } in modules.iter() {
            match module.query(&request.data, path.as_ref(), height, request.prove) {
                // success - implies query was handled by this module, so return response
                Ok(result) => {
                    let store = self.store.read().unwrap();
                    let proof_ops = if request.prove {
                        let proof = store.get_proof(height, &id.clone().into()).unwrap();
                        let mut buffer = Vec::new();
                        proof.encode(&mut buffer).unwrap(); // safety - cannot fail since buf is a vector

                        let mut ops = vec![];
                        if let Some(mut proofs) = result.proof {
                            ops.append(&mut proofs);
                        }
                        ops.push(ProofOp {
                            r#type: "".to_string(),
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
                        proof_ops,
                        height: store.current_height() as i64,
                        ..Default::default()
                    };
                }
                // `Error::NotHandled` - implies query isn't known or was intercepted but not
                // responded to by this module, so try with next module
                // todo(davirain)
                Err(e) if e.to_string() == "not handled" => continue,
                // Other error - return immediately
                Err(e) => return ResponseQuery::from_error(1, format!("query error: {e:?}")),
            }
        }
        ResponseQuery::from_error(1, "query msg not handled")
    }

    fn deliver_tx(&self, request: RequestDeliverTx) -> ResponseDeliverTx {
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
            match self.deliver_msg(message, &signer) {
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
                    for IdentifiedModule { module, .. } in modules.iter_mut() {
                        module.store_mut().reset();
                    }
                    self.store.write().unwrap().reset();
                    return ResponseDeliverTx::from_error(
                        2,
                        format!("deliver failed with error: {e}"),
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
        for IdentifiedModule { id, module } in modules.iter_mut() {
            module
                .store_mut()
                .commit()
                .expect("failed to commit to state");
            let mut state = self.store.write().unwrap();
            state
                .set(id.clone().into(), module.store().root_hash())
                .expect("failed to update sub-store commitment");
        }

        let mut state = self.store.write().unwrap();
        let data = state.commit().expect("failed to commit to state");
        info!(
            "Committed height {} with hash({})",
            state.current_height() - 1,
            data.iter().map(|b| format!("{b:02X}")).collect::<String>()
        );
        ResponseCommit {
            data: data.into(),
            retain_height: 0,
        }
    }

    fn begin_block(&self, request: RequestBeginBlock) -> ResponseBeginBlock {
        debug!("Got begin block request.");

        let mut modules = self.modules.write().unwrap();
        let mut events = vec![];
        let header = request.header.unwrap().try_into().unwrap();
        for IdentifiedModule { module, .. } in modules.iter_mut() {
            events.extend(module.begin_block(&header));
        }

        ResponseBeginBlock { events }
    }
}
