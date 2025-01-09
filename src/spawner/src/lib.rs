use std::borrow::Cow;

use candid::{ Principal, Encode };
use ic_cdk::update;
use miner::{ create_canister, install_code };

mod miner;

const BIL_CANISTER: &str = "hx36f-waaaa-aaaai-aq32q-cai";

pub fn miner_wasm() -> Cow<'static, [u8]> {
    Cow::Borrowed(include_bytes!(env!("MINER_WASM_PATH")))
}

#[update]
async fn spawn_miner(owner: Principal) -> Result<Principal, String> {
    if
        ic_cdk::caller() != Principal::from_text(BIL_CANISTER).unwrap()
    {
        return Err("not allowed".to_string());
    }

    let arg = Encode!(&owner).unwrap();

    let canister_id = create_canister(2_500_000_000_000).await.map_err(|e|
        format!("{} - {:?}", e.method, e.reason)
    )?;

    install_code(canister_id, miner_wasm().to_vec(), arg).await.map_err(|e|
        format!("{} - {:?}", e.method, e.reason)
    )?;

    Ok(canister_id)
}
