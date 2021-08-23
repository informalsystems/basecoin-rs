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
        pub mod v1beta1 {
            include!("prostgen/cosmos.base.v1beta1.rs");
        }

        pub mod query {
            pub mod v1beta1 {
                include!("prostgen/cosmos.base.query.v1beta1.rs");
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
    }
}
