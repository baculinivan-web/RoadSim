use roadsim_worker_protocol::EngineIdentity;

pub const ENGINE_VERSION: &str = "1.27.1";
pub const SOURCE_REVISION: &str = "7717f2379d9e314a0c81c5cec748444de06a2a91";

pub fn identity() -> EngineIdentity {
    EngineIdentity::new("eclipse.sumo", ENGINE_VERSION, SOURCE_REVISION)
        .expect("compile-time SUMO identity must be valid")
}
