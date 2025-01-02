use candid::{ CandidType, Principal };
use ic_cdk::update;
use rapidhash::RapidHasher;
use std::{ cell::RefCell, hash::Hasher };
use serde::{ Deserialize, Serialize };

const LEDGER_ID: &str = "hx36f-waaaa-aaaai-aq32q-cai";
const CHUNK_SIZE: u64 = 1000000; // 1M hashes

type Hash = u128;

#[derive(Debug, Clone, CandidType, Serialize, Deserialize)]
struct Transaction {
    sender: Principal,
    recipient: Principal,
    amount: u64,
    timestamp: u64,
}

#[derive(Debug, Clone, CandidType, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    transactions: Vec<Transaction>,
    nonce: u128,
    hash: Hash,
}

#[derive(Debug, Clone, CandidType, Serialize, Deserialize)]
pub struct BlockHeader {
    version: u32,
    pub height: u64,
    prev_hash: Hash,
    merkle_root: Hash,
    timestamp: u64,
    difficulty: u32,
}

#[update(hidden = true)]
pub async fn find_solution() {
    if
        ic_cdk::caller() != ic_cdk::api::id() &&
        ic_cdk::caller() != Principal::from_text(LEDGER_ID).unwrap()
    {
        ic_cdk::println!("Caller is not authorized!");
        return;
    }

    let seed = ic_cdk::api::time();
    let miner_id = read_state(|s| s.miner_id);
    let mut block = read_state(|s| s.current_block.clone().unwrap());

    ic_cdk::println!("Mining...");

    for i in 0..CHUNK_SIZE {
        block.nonce = xorshift_random(seed as u128, i as u128, miner_id as u128);

        let hash = calculate_hash(&block);

        if hash.leading_zeros() >= block.header.difficulty {
            if let Err(err) = submit_solution(block.clone()).await {
                ic_cdk::println!("Error submitting solution: {:?}", err);
            } else {
                ic_cdk::println!("Solution submitted successfully!");
                update_mining_stats(false);
                mutate_state(|s| {
                    s.blocks_mined += 1;
                });
                return;
            }
        }
    }

    let should_update_stats = mutate_state(|s| {
        s.mining_cycle += 1;
        s.mining_cycle % 5 == 0
    });

    if should_update_stats {
        update_mining_stats(true);
    }

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
                update_mining_stats(false);
            }
        }

    });
}

fn update_mining_stats(is_mining: bool) {
    mutate_state(|s| {
        s.time_spent_mining += ic_cdk::api::time() - s.mining_temp_time;
        s.cycles_burned += s.mining_temp_cycles - ic_cdk::api::canister_balance();
        s.is_mining = is_mining;
        s.last_mining_timestamp = ic_cdk::api::time();
        s.mining_temp_time = ic_cdk::api::time();
        s.mining_temp_cycles = ic_cdk::api::canister_balance();
    });
}

#[derive(Debug, Clone, CandidType, Serialize, Deserialize)]
struct Stats {
    cycles_burned: u64,
    timestamp: u64,
    solve_time: u64,
    miner: Principal,
}

async fn submit_solution(block: Block) -> Result<(), String> {
    let ledger_id = read_state(|s| s.ledger_id);
    let start_time = read_state(|s| s.mining_start_time);
    let start_cycles = read_state(|s| s.mining_start_cycles);

    let stats = Stats {
        cycles_burned: start_cycles - ic_cdk::api::canister_balance(),
        timestamp: ic_cdk::api::time(),
        solve_time: ic_cdk::api::time() - start_time,
        miner: ic_cdk::api::id(),
    };
    let res_gov: Result<(Result<bool, String>,), (i32, String)> = ic_cdk::api::call
        ::call(ledger_id, "submit_solution", (block, stats)).await
        .map_err(|(code, msg)| (code as i32, msg));
    match res_gov {
        Ok((res,)) =>
            match res {
                Ok(_) => Ok(()),
                Err(e) => Err(e),
            }
        Err((code, msg)) =>
            Err(format!("Error while calling minter canister ({}): {:?}", code, msg)),
    }
}

fn calculate_hash(block: &Block) -> Hash {
    let mut hasher = RapidHasher::new(0);
    let mut data = Vec::new();

    data.extend_from_slice(&block.header.version.to_le_bytes());
    data.extend_from_slice(&block.header.prev_hash.to_le_bytes());
    data.extend_from_slice(&block.header.merkle_root.to_le_bytes());
    data.extend_from_slice(&block.header.timestamp.to_le_bytes());
    data.extend_from_slice(&block.nonce.to_le_bytes());

    hasher.write(&data);
    let hash64 = hasher.finish();

    let hash128_high = {
        let mut hasher = RapidHasher::new(hash64);
        hasher.write(&hash64.to_le_bytes());
        hasher.finish()
    };

    ((hash128_high as u128) << 64) | (hash64 as u128)
}

fn xorshift_random(mut seed1: u128, mut seed2: u128, mut seed3: u128) -> u128 {
    seed1 ^= seed1 << 13;
    seed1 ^= seed1 >> 17;
    seed1 ^= seed1 << 5;

    seed2 ^= seed2 << 7;
    seed2 ^= seed2 >> 11;
    seed2 ^= seed2 << 3;

    seed3 ^= seed3 << 9;
    seed3 ^= seed3 >> 13;
    seed3 ^= seed3 << 7;

    let mix1 = seed1.rotate_left(32);
    let mix2 = seed2.rotate_right(29);
    let mix3 = seed3.rotate_left(37);

    mix1.wrapping_add(mix2).wrapping_add(mix3)
}
thread_local! {
    static __STATE: RefCell<Option<MinerState>> = RefCell::default();
}

#[derive(Clone, CandidType)]
pub struct MinerState {
    pub ledger_id: Principal,
    pub owner: Principal,
    pub cycles_burned: u64,
    pub blocks_mined: u64,
    pub last_mining_timestamp: u64,
    pub is_mining: bool,
    pub time_spent_mining: u64,
    pub mining_start_time: u64,
    pub mining_start_cycles: u64,
    pub mining_temp_time: u64,
    pub mining_temp_cycles: u64,
    pub current_block: Option<Block>,
    pub miner_id: u32,
    pub mining_cycle: u64,
}

impl MinerState {
    pub fn from_init(owner: Principal) -> Self {
        Self {
            ledger_id: Principal::from_text(LEDGER_ID).unwrap(),
            blocks_mined: 0,
            owner,
            cycles_burned: 0,
            is_mining: false,
            last_mining_timestamp: 0,
            time_spent_mining: 0,
            mining_start_time: 0,
            mining_start_cycles: 0,
            mining_temp_time: 0,
            mining_temp_cycles: 0,
            current_block: None,
            miner_id: 0,
            mining_cycle: 0,
        }
    }
}

pub fn mutate_state<F, R>(f: F) -> R where F: FnOnce(&mut MinerState) -> R {
    __STATE.with(|s| f(s.borrow_mut().as_mut().expect("State not initialized!")))
}

pub fn read_state<F, R>(f: F) -> R where F: FnOnce(&MinerState) -> R {
    __STATE.with(|s| f(s.borrow().as_ref().expect("State not initialized!")))
}

pub fn replace_state(state: MinerState) {
    __STATE.with(|s| {
        *s.borrow_mut() = Some(state);
    });
}
