use candid::{ CandidType, Principal };
use ic_stable_structures::cell::ValueError;
use ic_stable_structures::log::WriteError;
use ic_stable_structures::memory_manager::{ MemoryId, MemoryManager as MM, VirtualMemory };
use ic_stable_structures::storable::Bound;
use ic_stable_structures::DefaultMemoryImpl;
use ic_stable_structures::{
    DefaultMemoryImpl as DefMem,
    StableBTreeMap,
    StableLog,
    Storable,
    StableCell,
    StableVec,
};
use rapidhash::RapidHasher;
use serde::{ Deserialize, Serialize };
use std::borrow::Cow;
use std::cell::RefCell;
use std::hash::Hasher;
use crate::State;

pub type Hash = u128; // 128-bit hash

#[derive(Clone, CandidType, Ord, PartialOrd, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct TransactionArgs {
    pub recipient: Principal,
    pub amount: u64,
}

#[derive(Clone, CandidType, Ord, PartialOrd, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub sender: Principal,
    pub recipient: Principal,
    pub amount: u64,
    pub timestamp: u64,
}

#[derive(Clone, CandidType, Ord, PartialOrd, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct BlockHeader {
    pub version: u32,
    pub height: u64,
    pub prev_hash: Hash,
    pub merkle_root: Hash,
    pub timestamp: u64,
    pub difficulty: u32,
}

#[derive(Clone, CandidType, Ord, PartialOrd, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
    pub nonce: u128,
    pub hash: Hash,
}

impl Block {
    pub fn new(
        prev_block: &Block,
        transactions: Vec<Transaction>,
        difficulty: u32
    ) -> Result<Self, String> {
        let merkle_root = Block::calculate_merkle_root(&transactions);

        let header = BlockHeader {
            version: 1,
            height: prev_block.header.height + 1,
            prev_hash: prev_block.hash,
            merkle_root,
            timestamp: ic_cdk::api::time(),
            difficulty,
        };

        let mut block = Self {
            header,
            transactions,
            nonce: 0,
            hash: 0,
        };

        block.calculate_block_hash();

        Ok(block)
    }

    pub fn hash_transaction(transaction: &Transaction) -> Hash {
        let tx_string = format!(
            "{}{}{}",
            transaction.sender,
            transaction.recipient,
            transaction.amount
        );

        let mut hasher = RapidHasher::new(0);
        hasher.write(tx_string.as_bytes());
        let hash64 = hasher.finish();

        let hash128_high = {
            let mut hasher = RapidHasher::new(hash64);
            hasher.write(&hash64.to_le_bytes());
            hasher.finish()
        };

        ((hash128_high as u128) << 64) | (hash64 as u128)
    }

    pub fn calculate_merkle_root(transactions: &[Transaction]) -> Hash {
        if transactions.is_empty() {
            return 0;
        }

        let mut hashes: Vec<Hash> = transactions
            .iter()
            .map(|tx| Block::hash_transaction(tx))
            .collect();

        while hashes.len() > 1 {
            if hashes.len() % 2 != 0 {
                hashes.push(hashes.last().unwrap().clone());
            }

            hashes = hashes
                .chunks(2)
                .map(|chunk| {
                    let mut hasher = RapidHasher::new(0);
                    hasher.write(&chunk[0].to_le_bytes());
                    hasher.write(&chunk[1].to_le_bytes());
                    let hash64 = hasher.finish();

                    let hash128_high = {
                        let mut hasher = RapidHasher::new(hash64);
                        hasher.write(&hash64.to_le_bytes());
                        hasher.finish()
                    };

                    ((hash128_high as u128) << 64) | (hash64 as u128)
                })
                .collect();
        }

        hashes[0]
    }

    pub fn calculate_block_hash(&mut self) {
        let mut hasher = RapidHasher::new(0);
        hasher.write(&self.header.version.to_le_bytes());
        hasher.write(&self.header.height.to_le_bytes());
        hasher.write(&self.header.prev_hash.to_le_bytes());
        hasher.write(&self.header.merkle_root.to_le_bytes());
        hasher.write(&self.header.timestamp.to_le_bytes());
        hasher.write(&self.header.difficulty.to_le_bytes());
        hasher.write(&self.nonce.to_le_bytes());

        let hash64 = hasher.finish();
        let hash128_high = {
            let mut hasher = RapidHasher::new(hash64);
            hasher.write(&hash64.to_le_bytes());
            hasher.finish()
        };

        self.hash = ((hash128_high as u128) << 64) | (hash64 as u128);
    }

    pub fn genesis() -> Self {
        Self {
            header: BlockHeader {
                version: 1,
                height: 0,
                prev_hash: 0,
                merkle_root: 0,
                timestamp: 0,
                difficulty: 0,
            },
            transactions: vec![],
            nonce: 0,
            hash: 0,
        }
    }
}

#[derive(Clone, CandidType, Ord, PartialOrd, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct Stats {
    pub cycles_burned: u64,
    pub timestamp: u64,
    pub solve_time: u64,
    pub miner: Principal,
}

#[derive(Default, Ord, PartialOrd, Clone, Eq, PartialEq)]
struct Cbor<T>(pub T) where T: serde::Serialize + serde::de::DeserializeOwned;

impl<T> Storable for Cbor<T> where T: serde::Serialize + serde::de::DeserializeOwned {
    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        ciborium::ser::into_writer(&self.0, &mut buf).unwrap();
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Self(ciborium::de::from_reader(bytes.as_ref()).unwrap())
    }

    const BOUND: Bound = Bound::Unbounded;
}

const MINER_TO_OWNER_MEM_ID: MemoryId = MemoryId::new(0);
const TX_LOG_INDX_MEM_ID: MemoryId = MemoryId::new(1);
const TX_LOG_DATA_MEM_ID: MemoryId = MemoryId::new(2);
const CHAIN_INDX_MEM_ID: MemoryId = MemoryId::new(3);
const CHAIN_DATA_MEM_ID: MemoryId = MemoryId::new(4);
const USER_TO_BALANCE_MEM_ID: MemoryId = MemoryId::new(5);
const STATS_INDX_MEM_ID: MemoryId = MemoryId::new(6);
const STATS_DATA_MEM_ID: MemoryId = MemoryId::new(7);
const STATE_INDX_MEM_ID: MemoryId = MemoryId::new(8);
const STATE_DATA_MEM_ID: MemoryId = MemoryId::new(9);
const BURNED_EXE_MEM_ID: MemoryId = MemoryId::new(10);
const TRANSACTION_COUNT_MEM_ID: MemoryId = MemoryId::new(11);
const AVERAGE_BLOCK_TIME_MEM_ID: MemoryId = MemoryId::new(12);
const DIFFICULTY_MEM_ID: MemoryId = MemoryId::new(13);
const CURRENT_BLOCK_MEM_ID: MemoryId = MemoryId::new(14);
const USER_TO_BLOCK_MINED_MEM_ID: MemoryId = MemoryId::new(15);
const UPDATED_MINERS_MEM_ID: MemoryId = MemoryId::new(16);

type VM = VirtualMemory<DefMem>;

thread_local! {
    static MEMORY_MANAGER: RefCell<MM<DefaultMemoryImpl>> = RefCell::new(
        MM::init(DefaultMemoryImpl::default())
    );

    static MINER_TO_OWNER: RefCell<
        StableBTreeMap<Principal, (Principal, u64), VM>
    > = MEMORY_MANAGER.with(|mm| {
        RefCell::new(StableBTreeMap::init(mm.borrow().get(MINER_TO_OWNER_MEM_ID)))
    });

    static USER_TO_BALANCE: RefCell<StableBTreeMap<Principal, u64, VM>> = MEMORY_MANAGER.with(|mm| {
        RefCell::new(StableBTreeMap::init(mm.borrow().get(USER_TO_BALANCE_MEM_ID)))
    });

    static TX_LOG: RefCell<StableLog<Cbor<u64>, VM, VM>> = MEMORY_MANAGER.with(|mm| {
        RefCell::new(
            StableLog::init(
                mm.borrow().get(TX_LOG_INDX_MEM_ID),
                mm.borrow().get(TX_LOG_DATA_MEM_ID)
            ).expect("failed to initialize the block log")
        )
    });

    static STATS: RefCell<StableLog<Cbor<Stats>, VM, VM>> = MEMORY_MANAGER.with(|mm| {
        RefCell::new(
            StableLog::init(
                mm.borrow().get(STATS_INDX_MEM_ID),
                mm.borrow().get(STATS_DATA_MEM_ID)
            ).expect("failed to initialize the block log")
        )
    });

    static CHAIN: RefCell<StableLog<Cbor<Block>, VM, VM>> = MEMORY_MANAGER.with(|mm| {
        RefCell::new(
            StableLog::init(
                mm.borrow().get(CHAIN_INDX_MEM_ID),
                mm.borrow().get(CHAIN_DATA_MEM_ID)
            ).expect("failed to initialize the block log")
        )
    });

    static STATE: RefCell<StableLog<Cbor<State>, VM, VM>> = MEMORY_MANAGER.with(|mm| {
        RefCell::new(
            StableLog::init(
                mm.borrow().get(STATE_INDX_MEM_ID),
                mm.borrow().get(STATE_DATA_MEM_ID)
            ).expect("failed to initialize the block log")
        )
    });

    static BURNED_EXE: RefCell<StableCell<u64, VM>> = MEMORY_MANAGER.with(|mm| {
        RefCell::new(
            StableCell::init(mm.borrow().get(BURNED_EXE_MEM_ID), 0_u64).expect(
                "failed to initialize burned exe"
            )
        )
    });

    static TRANSACTION_COUNT: RefCell<StableCell<u64, VM>> = MEMORY_MANAGER.with(|mm| {
        RefCell::new(
            StableCell::init(mm.borrow().get(TRANSACTION_COUNT_MEM_ID), 0_u64).expect(
                "failed to initialize transaction count"
            )
        )
    });

    static AVERAGE_BLOCK_TIME: RefCell<StableCell<u64, VM>> = MEMORY_MANAGER.with(|mm| {
        RefCell::new(
            StableCell::init(mm.borrow().get(AVERAGE_BLOCK_TIME_MEM_ID), 0_u64).expect(
                "failed to initialize average block time"
            )
        )
    });

    static DIFFICULTY: RefCell<StableCell<u64, VM>> = MEMORY_MANAGER.with(|mm| {
        RefCell::new(
            StableCell::init(mm.borrow().get(DIFFICULTY_MEM_ID), 0_u64).expect(
                "failed to initialize difficulty"
            )
        )
    });

    static CURRENT_BLOCK: RefCell<StableBTreeMap<Cbor<Block>, (), VM>> = MEMORY_MANAGER.with(|mm| {
        RefCell::new(StableBTreeMap::init(mm.borrow().get(CURRENT_BLOCK_MEM_ID)))
    });

    static USER_TO_BLOCK_MINED: RefCell<StableBTreeMap<Principal, u64, VM>> = MEMORY_MANAGER.with(
        |mm| { RefCell::new(StableBTreeMap::init(mm.borrow().get(USER_TO_BLOCK_MINED_MEM_ID))) }
    );

    static UPDATED_MINERS: RefCell<StableVec<Principal, VM>> = MEMORY_MANAGER.with(|mm| {
        RefCell::new(
            StableVec::init(mm.borrow().get(UPDATED_MINERS_MEM_ID)).expect(
                "failed to initialize difficulty updated miners"
            )
        )
    });
}

pub fn add_burned_exe(amount: u64) -> Result<u64, ValueError> {
    let current = get_burned_exe();
    BURNED_EXE.with(|s| { s.borrow_mut().set(current + amount) })
}

pub fn get_burned_exe() -> u64 {
    BURNED_EXE.with(|s| *s.borrow().get())
}

pub fn update_transaction_count(amount: u64) -> Result<u64, ValueError> {
    let current = get_transaction_count();
    TRANSACTION_COUNT.with(|s| s.borrow_mut().set(current + amount))
}

pub fn get_transaction_count() -> u64 {
    TRANSACTION_COUNT.with(|s| *s.borrow().get())
}

pub fn update_average_block_time(amount: u64) -> Result<u64, ValueError> {
    AVERAGE_BLOCK_TIME.with(|s| s.borrow_mut().set(amount))
}

pub fn get_average_block_time() -> u64 {
    AVERAGE_BLOCK_TIME.with(|s| *s.borrow().get())
}

pub fn update_difficulty(amount: u64) -> Result<u64, ValueError> {
    DIFFICULTY.with(|s| s.borrow_mut().set(amount))
}

pub fn difficulty() -> u64 {
    DIFFICULTY.with(|s| *s.borrow().get())
}

pub fn current_block() -> Vec<Block> {
    CURRENT_BLOCK.with(|s|
        s
            .borrow()
            .iter()
            .map(|(k, _)| k.0)
            .collect()
    )
}

pub fn update_current_block(block: Block) {
    CURRENT_BLOCK.with(|s| {
        s.borrow_mut().clear_new();
        s.borrow_mut().insert(Cbor(block), ())
    });
}

pub fn get_users_to_block_mined() -> Vec<(Principal, u64)> {
    USER_TO_BLOCK_MINED.with(|s| s.borrow().iter().collect())
}

pub fn add_block_mined(user: Principal) {
    USER_TO_BLOCK_MINED.with(|s| {
        let new_balance = s.borrow().get(&user).unwrap_or(0) + 1;
        s.borrow_mut().insert(user, new_balance);
    });
}

pub fn add_updated_miner(miner: Principal) -> Result<(), ic_stable_structures::GrowFailed> {
    UPDATED_MINERS.with(|s| {
        s.borrow_mut().push(&miner)
    })
}

pub fn get_all_updated_miners() -> Vec<Principal> {
    UPDATED_MINERS.with(|s| {
        let all_elements: std::vec::Vec<Principal> = s.borrow().iter().collect();
        all_elements
    })
}

pub fn insert_block(block: Block) -> Result<u64, WriteError> {
    CHAIN.with(|s| s.borrow_mut().append(&Cbor(block)))
}

pub fn all_blocks() -> Vec<Block> {
    CHAIN.with(|s|
        s
            .borrow()
            .iter()
            .map(|b| b.0)
            .collect()
    )
}

pub fn latest_block() -> Option<Block> {
    CHAIN.with(|s|
        s
            .borrow()
            .iter()
            .last()
            .map(|b| b.0)
    )
}

pub fn block_count() -> u64 {
    CHAIN.with(|s| s.borrow().len())
}

pub fn get_block(index: u64) -> Option<Block> {
    CHAIN.with(|s|
        s
            .borrow()
            .get(index)
            .map(|b| b.0)
    )
}

pub fn insert_new_transaction(block: u64) -> Result<u64, WriteError> {
    TX_LOG.with(|s| s.borrow_mut().append(&Cbor(block)))
}

pub fn get_all_transactions() -> Vec<u64> {
    TX_LOG.with(|s|
        s
            .borrow()
            .iter()
            .map(|b| b.0)
            .collect()
    )
}

pub fn get_transaction(index: u64) -> Option<u64> {
    TX_LOG.with(|s|
        s
            .borrow()
            .get(index)
            .map(|b| b.0)
    )
}

pub fn transaction_count() -> u64 {
    TX_LOG.with(|s| s.borrow().len())
}

pub fn insert_new_miner(miner: Principal, owner: Principal, block_index: u64) {
    MINER_TO_OWNER.with(|s| s.borrow_mut().insert(miner, (owner, block_index)));
}

pub fn get_miner_owner(miner: Principal) -> Option<Principal> {
    MINER_TO_OWNER.with(|s|
        s
            .borrow()
            .get(&miner)
            .map(|(owner, _)| owner)
    )
}

pub fn miner_count() -> u64 {
    MINER_TO_OWNER.with(|s| s.borrow().len())
}

pub fn get_miner_to_owner_and_index() -> Vec<(Principal, (Principal, u64))> {
    MINER_TO_OWNER.with(|s| s.borrow().iter().collect())
}

pub fn add_balance(user: Principal, amount: u64) {
    USER_TO_BALANCE.with(|s| {
        let new_balance = s.borrow().get(&user).unwrap_or(0) + amount;
        s.borrow_mut().insert(user, new_balance);
    });
}

pub fn sub_balance(user: Principal, amount: u64) {
    USER_TO_BALANCE.with(|s| {
        let new_balance = s.borrow().get(&user).unwrap_or(0).saturating_sub(amount);
        s.borrow_mut().insert(user, new_balance);
    });
}

pub fn get_balance(user: Principal) -> u64 {
    USER_TO_BALANCE.with(|s| s.borrow().get(&user).unwrap_or(0))
}

pub fn insert_stats(stats: Stats) -> Result<u64, WriteError> {
    STATS.with(|s| s.borrow_mut().append(&Cbor(stats)))
}

pub fn get_stat(index: u64) -> Option<Stats> {
    STATS.with(|s|
        s
            .borrow()
            .get(index)
            .map(|b| b.0)
    )
}

pub fn all_stats() -> Vec<Stats> {
    STATS.with(|s|
        s
            .borrow()
            .iter()
            .map(|b| b.0)
            .collect()
    )
}

pub fn get_stats(index: u64) -> Option<Stats> {
    STATS.with(|s|
        s
            .borrow()
            .get(index)
            .map(|b| b.0)
    )
}

pub fn add_state(state: State) -> Result<u64, WriteError> {
    STATE.with(|s| s.borrow_mut().append(&Cbor(state)))
}

pub fn get_last_state() -> Option<State> {
    STATE.with(|s|
        s
            .borrow()
            .iter()
            .last()
            .map(|b| b.0)
    )
}
