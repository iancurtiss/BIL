use candid::{ CandidType, Principal };
use ic_base_types::PrincipalId;
use ic_management_canister_types::{
    CanisterIdRecord,
    CanisterInstallMode,
    CanisterSettingsArgs,
    CreateCanisterArgs,
    InstallCodeArgs,
    LogVisibilityV2,
};
use ic_cdk::api::call::RejectionCode;
use serde::de::DeserializeOwned;

#[derive(Debug, Clone, PartialEq, Eq, CandidType)]
pub struct CallError {
    pub method: String,
    pub reason: Reason,
}

#[derive(Debug, Clone, PartialEq, Eq, CandidType)]
pub enum Reason {
    OutOfCycles,
    CanisterError(String),
    Rejected(String),
    TransientInternalError(String),
    InternalError(String),
}

impl Reason {
    fn from_reject(reject_code: RejectionCode, reject_message: String) -> Self {
        match reject_code {
            RejectionCode::SysTransient => Self::TransientInternalError(reject_message),
            RejectionCode::CanisterError => Self::CanisterError(reject_message),
            RejectionCode::CanisterReject => Self::Rejected(reject_message),
            | RejectionCode::NoError
            | RejectionCode::SysFatal
            | RejectionCode::DestinationInvalid
            | RejectionCode::Unknown =>
                Self::InternalError(
                    format!(
                        "rejection code: {:?}, rejection message: {}",
                        reject_code,
                        reject_message
                    )
                ),
        }
    }
}

async fn call<I, O>(method: &str, payment: u64, input: &I) -> Result<O, CallError>
    where I: CandidType, O: CandidType + DeserializeOwned
{
    let balance = ic_cdk::api::canister_balance128();
    if balance < (payment as u128) {
        return Err(CallError {
            method: method.to_string(),
            reason: Reason::OutOfCycles,
        });
    }

    let res: Result<(O,), _> = ic_cdk::api::call::call_with_payment(
        Principal::management_canister(),
        method,
        (input,),
        payment
    ).await;

    match res {
        Ok((output,)) => Ok(output),
        Err((code, msg)) =>
            Err(CallError {
                method: method.to_string(),
                reason: Reason::from_reject(code, msg),
            }),
    }
}

pub async fn install_code(
    canister_id: Principal,
    wasm_module: Vec<u8>,
    arg: Vec<u8>
) -> Result<(), CallError> {
    let install_code = InstallCodeArgs {
        mode: CanisterInstallMode::Install,
        canister_id: PrincipalId::from(canister_id),
        wasm_module,
        arg,
        compute_allocation: None,
        memory_allocation: None,
        sender_canister_version: None,
    };

    call("install_code", 0, &install_code).await?;

    Ok(())
}

pub async fn create_canister(cycles_for_canister_creation: u64) -> Result<Principal, CallError> {
    let create_args = CreateCanisterArgs {
        settings: Some(CanisterSettingsArgs {
            compute_allocation: None,
            memory_allocation: None,
            freezing_threshold: None,
            controllers: None,
            reserved_cycles_limit: None,
            log_visibility: Some(LogVisibilityV2::Public),
            wasm_memory_limit: None,
            wasm_memory_threshold: None,
        }),
        ..Default::default()
    };
    let result: CanisterIdRecord = call(
        "create_canister", // provisional_create_canister_with_cycles for local testing
        cycles_for_canister_creation,
        &create_args
    ).await?;

    Ok(result.get_canister_id().get().into())
}
