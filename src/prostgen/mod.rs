#[allow(unused)]
pub const COSMOS_SDK_VERSION: &str = include_str!("COSMOS_SDK_COMMIT");

pub mod cosmos {
    pub mod auth {
        pub mod v1beta1 {
            include!("cosmos.auth.v1beta1.rs");
        }
    }

    pub mod staking {
        pub mod v1beta1 {
            include!("cosmos.staking.v1beta1.rs");
        }
    }

    pub mod upgrade {
        pub mod v1beta1 {
            include!("cosmos.upgrade.v1beta1.rs");
        }
    }

    pub mod base {
        pub mod tendermint {
            pub mod v1beta1 {
                include!("cosmos.base.tendermint.v1beta1.rs");
            }
        }

        pub mod v1beta1 {
            include!("cosmos.base.v1beta1.rs");
        }

        pub mod query {
            pub mod v1beta1 {
                include!("cosmos.base.query.v1beta1.rs");
            }
        }

        pub mod abci {
            pub mod v1beta1 {
                include!("cosmos.base.abci.v1beta1.rs");
            }
        }
    }

    pub mod crypto {
        pub mod multisig {
            pub mod v1beta1 {
                include!("cosmos.crypto.multisig.v1beta1.rs");
            }
        }
    }

    pub mod tx {
        pub mod v1beta1 {
            include!("cosmos.tx.v1beta1.rs");
        }

        pub mod signing {
            pub mod v1beta1 {
                include!("cosmos.tx.signing.v1beta1.rs");
            }
        }
    }
}

pub mod ibc {
    pub mod core {
        pub mod client {
            pub mod v1 {
                include!("ibc.core.client.v1.rs");
            }
        }

        pub mod commitment {
            pub mod v1 {
                include!("ibc.core.commitment.v1.rs");
            }
        }

        pub mod connection {
            pub mod v1 {
                include!("ibc.core.connection.v1.rs");
            }
        }

        pub mod channel {
            pub mod v1 {
                include!("ibc.core.channel.v1.rs");
            }
        }

        pub mod port {
            pub mod v1 {
                include!("ibc.core.port.v1.rs");
            }
        }
    }
}

pub mod ics23 {
    include!("ics23.rs");
}
