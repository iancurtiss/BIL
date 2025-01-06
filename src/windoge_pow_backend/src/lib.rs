use candid::{ CandidType, Principal };
use memory::{block_count, Block, Transaction};
use serde::{ Deserialize, Serialize };
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{ BTreeMap, BTreeSet };

const COINBASE_REWARDS: u64 = 60_000_000_000;
const INIT_DIFFICULTY: u32 = 26;
pub const BLOCK_HALVING: u64 = 17_500;
pub const SEC_NANOS: u64 = 1_000_000_000;
pub const BIL_LEDGER_ID: &str = "ktra4-taaaa-aaaag-atveq-cai";

pub mod memory;
pub mod miner;

#[derive(Debug, Clone)]
pub struct MinerWasm;

pub fn miner_wasm() -> Cow<'static, [u8]> {
    Cow::Borrowed(include_bytes!(env!("MINER_WASM_PATH")))
}

thread_local! {
    static __STATE: RefCell<Option<State>> = RefCell::default();
}

#[derive(Clone, CandidType, Ord, PartialOrd, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct State {
    pub current_difficulty: u32,

    pub transaction_count: u64,

    pub block_height: u64,

    pub exe_burned: u64,

    pub average_block_time: u64,

    pub current_block: Option<Block>,

    pub bil_ledger_id: Principal,

    pub miner_to_burned_cycles: BTreeMap<Principal, u64>,

    pub miner_to_mined_block: BTreeMap<Principal, u64>,

    pub principal_to_miner: BTreeMap<Principal, Vec<Principal>>,
    
    pub miner_to_owner: BTreeMap<Principal, Principal>,

    pub miner_creation_transactions: BTreeSet<u64>,

    pub mempool: Vec<Transaction>,

    pub pending_balance: BTreeMap<Principal, u64>,

    pub updated_miners: Vec<Principal>
}

impl State {
    pub fn new() -> Self {
        Self {
            current_difficulty: INIT_DIFFICULTY,

            transaction_count: 0,

            block_height: 0,

            exe_burned: 0,

            average_block_time: 0,

            current_block: None,

            bil_ledger_id: Principal::from_text(BIL_LEDGER_ID).unwrap(),

            miner_to_burned_cycles: BTreeMap::default(),

            miner_to_mined_block: BTreeMap::default(),

            principal_to_miner: BTreeMap::default(),
            miner_to_owner: BTreeMap::default(),

            miner_creation_transactions: BTreeSet::default(),

            mempool: Vec::new(),

            pending_balance: BTreeMap::default(),

            updated_miners: Vec::new()
        }
    }

    pub fn new_miner(&mut self, miner: Principal, caller: Principal, block_index: u64) {
        self.miner_creation_transactions.insert(block_index);
        self.miner_to_owner.insert(miner, caller);
        self.principal_to_miner.entry(caller).or_default().push(miner);
    }

    pub fn current_rewards(&self) -> u64 {
        COINBASE_REWARDS >> (block_count() / BLOCK_HALVING)
    }
}

pub fn mutate_state<F, R>(f: F) -> R where F: FnOnce(&mut State) -> R {
    __STATE.with(|s| f(s.borrow_mut().as_mut().expect("State not initialized!")))
}

pub fn read_state<F, R>(f: F) -> R where F: FnOnce(&State) -> R {
    __STATE.with(|s| f(s.borrow().as_ref().expect("State not initialized!")))
}

pub fn replace_state(state: State) {
    __STATE.with(|s| {
        *s.borrow_mut() = Some(state);
    });
}