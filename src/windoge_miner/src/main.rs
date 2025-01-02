use windoge_miner::{ mutate_state, read_state, replace_state, Block, MinerState };
use candid::Principal;
use ic_cdk::{ init, query, update };
use ic_cdk::api::call::{msg_cycles_available128, msg_cycles_accept128};

fn main() {}

#[init]
fn init(owner: Principal) {
    replace_state(MinerState::from_init(owner));
}

#[query]
fn cycles_left() -> u64 {
    ic_cdk::api::canister_balance()
}

#[update]
fn receive() {
    let available_cycles: u128 = msg_cycles_available128();
    let accepted_cycles: u128 = msg_cycles_accept128(available_cycles);

    mutate_state(|s| {
        s.mining_start_cycles += accepted_cycles as u64;
    });

    ic_cdk::println!("Received cycles: {}", accepted_cycles);
}

#[update(hidden = true)]
async fn push_block(block: Block, miner_id: u32) {
    let ledger_id = read_state(|s| s.ledger_id);
    assert_eq!(ic_cdk::caller(), ledger_id);

    let should_start_mining = read_state(|s| !s.is_mining);
    
    mutate_state(|s| {
        s.current_block = Some(block.clone());
        s.is_mining = true;
        s.last_mining_timestamp = ic_cdk::api::time();
        s.mining_start_cycles = ic_cdk::api::canister_balance();
        s.mining_start_time = ic_cdk::api::time();
        s.mining_temp_time = ic_cdk::api::time();
        s.mining_temp_cycles = ic_cdk::api::canister_balance();
        s.miner_id = miner_id;
        s.mining_cycle = 0;
    });

    ic_cdk::println!("New block received: {:?}", block.header.height);

    if should_start_mining {
        ic_cdk::println!("Starting mining...");
        ic_cdk::spawn(async {
            match ic_cdk::api::call::call_raw(
                ic_cdk::api::id(),
                "find_solution",
                candid::encode_args(()).unwrap(),
                0
            ).await {
                Ok(_) => (),
                Err(err) => {
                    ic_cdk::println!("Error calling self: {:?}", err);
                    mutate_state(|s| {
                        s.is_mining = false;
                    });
                }
            }
        });
    }
}

#[query]
fn get_state() -> MinerState {
    read_state(|s| s.clone())
}
