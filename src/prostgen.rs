#[allow(unused)]
pub const COSMOS_SDK_VERSION: &str = include_str!("prostgen/COSMOS_SDK_COMMIT");

pub mod cosmos {
    pub mod auth {
        pub mod v1beta1 {
            include!("prostgen/cosmos.auth.v1beta1.rs");
        }
    }

    pub mod staking {
        pub mod v1beta1 {
            include!("prostgen/cosmos.staking.v1beta1.rs");
        }
    }

    pub mod upgrade {
        pub mod v1beta1 {
            include!("prostgen/cosmos.upgrade.v1beta1.rs");
        }
    }

    pub mod base {
        pub mod tendermint {
            pub mod v1beta1 {
                include!("prostgen/cosmos.base.tendermint.v1beta1.rs");
            }
        }

        pub mod v1beta1 {
            include!("prostgen/cosmos.base.v1beta1.rs");
        }

        pub mod query {
            pub mod v1beta1 {
                include!("prostgen/cosmos.base.query.v1beta1.rs");
            }
        }

        pub mod abci {
            pub mod v1beta1 {
                include!("prostgen/cosmos.base.abci.v1beta1.rs");
            }
        }
    }

    pub mod crypto {
        pub mod multisig {
            pub mod v1beta1 {
                include!("prostgen/cosmos.crypto.multisig.v1beta1.rs");
            }
        }
    }

    pub mod tx {
        pub mod v1beta1 {
            include!("prostgen/cosmos.tx.v1beta1.rs");
        }

        pub mod signing {
            pub mod v1beta1 {
                include!("prostgen/cosmos.tx.signing.v1beta1.rs");
            }
        }
    }
}

pub mod ibc {
    pub mod core {
        pub mod client {
            pub mod v1 {
                include!("prostgen/ibc.core.client.v1.rs");
            }
        }

        pub mod commitment {
            pub mod v1 {
                include!("prostgen/ibc.core.commitment.v1.rs");
            }
        }

        pub mod connection {
            pub mod v1 {
                include!("prostgen/ibc.core.connection.v1.rs");
            }
        }

        pub mod channel {
            pub mod v1 {
                include!("prostgen/ibc.core.channel.v1.rs");
            }
        }

        pub mod port {
            pub mod v1 {
                include!("prostgen/ibc.core.port.v1.rs");
            }
        }
    }
}

pub mod ics23 {
    include!("prostgen/ics23.rs");
}
