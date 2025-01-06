use ic_cdk_timers::TimerId;
use windoge_miner::{ mutate_state, read_state, replace_state, Block, MinerState };
use candid::{ Decode, Principal };
use ic_cdk::{ init, post_upgrade, query, update };
use ic_cdk::api::call::{ msg_cycles_available128, msg_cycles_accept128 };
use std::cell::RefCell;

thread_local! {
    static TIMER_ID: RefCell<Vec<TimerId>> = RefCell::new(Vec::new());
}

fn main() {}

#[init]
fn init(owner: Principal) {
    replace_state(MinerState::from_init(owner));

    start_timer();
}

#[post_upgrade]
fn post_upgrade(owner: Principal) {
    replace_state(MinerState::from_init(owner));

    start_timer();
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
        s.mining_temp_cycles += accepted_cycles as u64;
    });

    let timer_id = TIMER_ID.with(|s| s.borrow().get(0).cloned());
    if let Some(id) = timer_id {
        ic_cdk_timers::clear_timer(id);
        TIMER_ID.with(|s| {
            let mut timers = s.borrow_mut();
            if !timers.is_empty() {
                timers.pop();
            }
        });
    }

    start_timer();

    ic_cdk::println!("Received cycles!: {}", accepted_cycles);
}

fn start_timer() {
    let timer_id = ic_cdk_timers::set_timer_interval(std::time::Duration::from_secs(60), || {
        ic_cdk::spawn(async {
            let _ = update_block().await;
        });
    });

    TIMER_ID.with(|s| s.borrow_mut().push(timer_id))
}

async fn new_block_found(block: Block) -> Result<(), String> {
    ic_cdk::println!("New block received: {:?}", block.header.height);

    let should_start_mining = read_state(|s| !s.is_mining);

    mutate_state(|s| {
        s.current_block = Some(block.clone());
        s.is_mining = true;
        s.last_mining_timestamp = ic_cdk::api::time();
        s.mining_start_cycles = ic_cdk::api::canister_balance();
        s.mining_start_time = ic_cdk::api::time();
        s.mining_temp_time = ic_cdk::api::time();
        s.mining_temp_cycles = ic_cdk::api::canister_balance();
        s.miner_id = (ic_cdk::api::time() % 300) as u32; // random miner id
        s.mining_cycle = 0;
    });

    if should_start_mining {
        ic_cdk::println!("Starting mining...");
        ic_cdk::spawn(async {
            match
                ic_cdk::api::call::call_raw(
                    ic_cdk::api::id(),
                    "find_solution",
                    candid::encode_args(()).unwrap(),
                    0
                ).await
            {
                Ok(_) => (),
                Err(err) => {
                    ic_cdk::println!("Error calling find_solution: {:?}", err);
                    mutate_state(|s| {
                        s.is_mining = false;
                    });
                }
            }
        });
    }

    Ok(())
}

async fn update_block() -> Result<(), String> {
    let ledger_id = read_state(|s| s.ledger_id);
    match
        ic_cdk::api::call::call_raw(
            ledger_id,
            "get_current_block",
            candid::encode_args(()).unwrap(),
            0
        ).await
    {
        Ok(res) => {
            let res_block = Decode!(&res, Option<Block>).map_err(|e| format!("{:?}", e))?;
            ic_cdk::println!("Updating the block...");
            if let Some(block) = res_block {
                let res_current_block = read_state(|s| s.current_block.clone());
                if let Some(current_block) = res_current_block {
                    if current_block != block {
                        let _ = new_block_found(block.clone()).await;
                    }
                } else {
                    let _ = new_block_found(block.clone()).await;
                }
            }
            return Ok(());
        }
        Err(err) => {
            ic_cdk::println!("get_current_block error: {:?}", err);
            return Err(err.1);
        }
    };
}

#[query]
fn get_state() -> MinerState {
    read_state(|s| s.clone())
}
