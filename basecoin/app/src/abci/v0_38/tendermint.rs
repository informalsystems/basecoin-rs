use std::fmt::Debug;

use basecoin_modules::error::Error;
use basecoin_modules::types::IdentifiedModule;
use basecoin_store::context::{ProvableStore, Store};
use basecoin_store::types::{Height, Path};
use basecoin_store::utils::SharedRwExt;
use prost::Message;
use serde_json::Value;
use tendermint::merkle::proof::{ProofOp, ProofOps};
use tendermint_abci::Application;
use tendermint_proto::abci::{
    RequestFinalizeBlock, RequestInfo, RequestInitChain, RequestQuery, ResponseCommit,
    ResponseFinalizeBlock, ResponseInfo, ResponseInitChain, ResponseQuery,
};
use tracing::{debug, info};

use crate::builder::BaseCoinApp;
use crate::error::ResponseFromErrorExt;

impl<S: Debug + ProvableStore> Application for BaseCoinApp<S> {
    fn info(&self, request: RequestInfo) -> ResponseInfo {
        let (last_block_height, last_block_app_hash) = {
            let state = self.store.read_access();
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
        debug!("Got init chain request.{:?}", request);
        // safety - we panic on errors to prevent chain creation with invalid genesis config
        let app_state: Value = serde_json::from_str(
            &String::from_utf8(request.app_state_bytes.clone().into())
                .expect("invalid genesis state"),
        )
        .expect("genesis state isn't valid JSON");

        let mut modules = self.modules.write_access();

        for IdentifiedModule { module, .. } in modules.iter_mut() {
            module.init(app_state.clone());
        }

        info!("App initialized");

        ResponseInitChain {
            consensus_params: request.consensus_params,
            validators: vec![], // use validator set proposed by tendermint (ie. in the genesis file)
            app_hash: self.store.write_access().root_hash().into(),
        }
    }

    fn query(&self, request: RequestQuery) -> ResponseQuery {
        debug!("Got query request: {:?}", request);

        let path: Option<Path> = request.path.try_into().ok();

        let modules = self.modules.read_access();

        let height = Height::from(request.height as u64);

        for IdentifiedModule { id, module } in modules.iter() {
            match module.query(&request.data, path.as_ref(), height, request.prove) {
                // success - implies query was handled by this module, so return response
                Ok(result) => {
                    let store = self.store.read_access();
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
                        proof_ops: match proof_ops {
                            Some(proof_ops) => Some(proof_ops.into()),
                            None => None,
                        },
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

    fn commit(&self) -> ResponseCommit {
        let mut modules = self.modules.write_access();

        for IdentifiedModule { id, module } in modules.iter_mut() {
            module
                .store_mut()
                .commit()
                .expect("failed to commit to state");
            let mut state = self.store.write_access();
            state
                .set(id.clone().into(), module.store().root_hash())
                .expect("failed to update sub-store commitment");
        }

        let mut state = self.store.write_access();

        let data = state.commit().expect("failed to commit to state");

        info!(
            "Committed height {} with hash({})",
            state.current_height() - 1,
            data.iter().map(|b| format!("{b:02X}")).collect::<String>()
        );
        ResponseCommit { retain_height: 0 }
    }

    fn finalize_block(&self, _request: RequestFinalizeBlock) -> ResponseFinalizeBlock {
        unimplemented!()
    }
}
