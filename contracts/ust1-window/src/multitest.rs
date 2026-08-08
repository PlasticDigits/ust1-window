//! cw-multi-test coverage for **INV-LIMIT-001**, **INV-SWAP-001**, **INV-WITHDRAW-001/002**,
//! pause/ACL, and failure paths. Uses a stub treasury that accepts `InstantWithdrawCw20`
//! (no CW20 allowance).

use cosmwasm_std::{
    to_json_binary, Binary, Deps, DepsMut, Empty, Env, MessageInfo, Response as CwResponse,
    StdResult, Addr, Timestamp, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, MinterResponse};
use cw_multi_test::{App, ContractWrapper, Executor};

use crate::msg::{ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::treasury::TreasuryExecuteMsg;
use ust1_common::{
    DEFAULT_FEE_BPS, DEFAULT_MAX_ORACLE_AGE_SECS, DEFAULT_PER_TX_UST1_LIMIT,
    DEFAULT_ROLLING_24H_UST1_LIMIT, MIN_ORACLE_UPDATE_INTERVAL_SECS, RATE_SCALE,
};
use ust1_oracle::msg as oracle_msg;

/// Minimal treasury stub: holds CW20 and honors `InstantWithdrawCw20` with a Transfer.
/// Optional reject mode simulates unregistered / paused treasury for atomicity tests.
mod stub_treasury {
    use super::*;

    #[cosmwasm_schema::cw_serde]
    pub struct InstantiateMsg {
        /// When true, InstantWithdrawCw20 always fails (spender not registered / paused).
        pub reject_pulls: bool,
    }

    #[cosmwasm_schema::cw_serde]
    pub enum ExecuteMsg {
        InstantWithdrawCw20 {
            recipient: String,
            token: String,
            amount: Uint128,
        },
        /// Test-only: toggle reject mode after instantiate.
        SetRejectPulls { reject: bool },
    }

    pub fn instantiate(
        deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        msg: InstantiateMsg,
    ) -> StdResult<CwResponse> {
        deps.storage.set(b"reject", &[u8::from(msg.reject_pulls)]);
        Ok(CwResponse::new().add_attribute("action", "stub_treasury_instantiate"))
    }

    pub fn execute(
        deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        msg: ExecuteMsg,
    ) -> StdResult<CwResponse> {
        match msg {
            ExecuteMsg::SetRejectPulls { reject } => {
                deps.storage.set(b"reject", &[u8::from(reject)]);
                Ok(CwResponse::new().add_attribute("action", "set_reject_pulls"))
            }
            ExecuteMsg::InstantWithdrawCw20 {
                recipient,
                token,
                amount,
            } => {
                let reject = deps.storage.get(b"reject").map(|v| v[0] != 0).unwrap_or(false);
                if reject {
                    return Err(cosmwasm_std::StdError::generic_err(
                        "stub treasury: InstantWithdrawCw20 rejected (unregistered or paused)",
                    ));
                }
                Ok(CwResponse::new()
                    .add_message(WasmMsg::Execute {
                        contract_addr: token,
                        msg: to_json_binary(&Cw20ExecuteMsg::Transfer { recipient, amount })?,
                        funds: vec![],
                    })
                    .add_attribute("action", "instant_withdraw_cw20"))
            }
        }
    }

    pub fn query(_deps: Deps, _env: Env, _msg: Empty) -> StdResult<Binary> {
        to_json_binary(&Empty {})
    }
}

fn oracle_contract() -> Box<dyn cw_multi_test::Contract<Empty>> {
    let c = ContractWrapper::new(
        ust1_oracle::contract::execute,
        ust1_oracle::contract::instantiate,
        ust1_oracle::contract::query,
    )
    .with_migrate(ust1_oracle::contract::migrate);
    Box::new(c)
}

fn window_contract() -> Box<dyn cw_multi_test::Contract<Empty>> {
    let c = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_migrate(crate::contract::migrate);
    Box::new(c)
}

fn cw20_mintable_contract() -> Box<dyn cw_multi_test::Contract<Empty>> {
    let c = ContractWrapper::new(
        cw20_mintable::contract::execute,
        cw20_mintable::contract::instantiate,
        cw20_mintable::contract::query,
    );
    Box::new(c)
}

fn stub_treasury_contract() -> Box<dyn cw_multi_test::Contract<Empty>> {
    let c = ContractWrapper::new(
        stub_treasury::execute,
        stub_treasury::instantiate,
        stub_treasury::query,
    );
    Box::new(c)
}

struct TestEnv {
    app: App,
    owner: Addr,
    user: Addr,
    treasury: Addr,
    vfdusd: Addr,
    ust1: Addr,
    window: Addr,
    window_code_id: u64,
}

fn setup() -> TestEnv {
    setup_with_treasury(false)
}

fn setup_with_treasury(reject_pulls: bool) -> TestEnv {
    let mut app = App::default();
    let owner = Addr::unchecked("owner");
    let bot = Addr::unchecked("bot");
    let user = Addr::unchecked("user");

    let oracle_id = app.store_code(oracle_contract());
    let window_id = app.store_code(window_contract());
    let cw20_id = app.store_code(cw20_mintable_contract());
    let treasury_id = app.store_code(stub_treasury_contract());

    let oracle = app
        .instantiate_contract(
            oracle_id,
            owner.clone(),
            &oracle_msg::InstantiateMsg {
                governance: owner.to_string(),
                oracle_operator: bot.to_string(),
                initial_rate: Uint128::from(RATE_SCALE),
            },
            &[],
            "oracle",
            None,
        )
        .unwrap();

    let vfdusd = app
        .instantiate_contract(
            cw20_id,
            owner.clone(),
            &cw20_mintable::msg::InstantiateMsg {
                name: "vFDUSD".into(),
                symbol: "vFDUSD".into(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: owner.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            &[],
            "vfdusd",
            None,
        )
        .unwrap();

    let ust1 = app
        .instantiate_contract(
            cw20_id,
            owner.clone(),
            &cw20_mintable::msg::InstantiateMsg {
                name: "UST1".into(),
                symbol: "UST1".into(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: owner.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            &[],
            "ust1",
            None,
        )
        .unwrap();

    let treasury = app
        .instantiate_contract(
            treasury_id,
            owner.clone(),
            &stub_treasury::InstantiateMsg { reject_pulls },
            &[],
            "treasury",
            None,
        )
        .unwrap();

    let window = app
        .instantiate_contract(
            window_id,
            owner.clone(),
            &InstantiateMsg {
                governance: owner.to_string(),
                oracle: oracle.to_string(),
                vfdusd_token: vfdusd.to_string(),
                cmm_treasury: Some(treasury.to_string()),
                ust1_token: ust1.to_string(),
                fee_bps: DEFAULT_FEE_BPS,
                per_tx_ust1_limit: Uint128::from(DEFAULT_PER_TX_UST1_LIMIT),
                rolling_24h_ust1_limit: Uint128::from(DEFAULT_ROLLING_24H_UST1_LIMIT),
                max_oracle_age_sec: None,
            },
            &[],
            "window",
            Some(owner.to_string()),
        )
        .unwrap();

    let st: oracle_msg::StateResponse = app
        .wrap()
        .query_wasm_smart(&oracle, &oracle_msg::QueryMsg::State {})
        .unwrap();
    app.execute_contract(
        bot.clone(),
        oracle.clone(),
        &oracle_msg::ExecuteMsg::UpdateRate { new_rate: st.rate },
        &[],
    )
    .unwrap();

    app.execute_contract(
        owner.clone(),
        ust1.clone(),
        &cw20_mintable::msg::ExecuteMsg::AddMinter {
            minter: window.to_string(),
        },
        &[],
    )
    .unwrap();

    TestEnv {
        app,
        owner,
        user,
        treasury,
        vfdusd,
        ust1,
        window,
        window_code_id: window_id,
    }
}

fn cw20_balance(app: &App, token: &Addr, address: &Addr) -> Uint128 {
    let bal: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            token,
            &cw20::Cw20QueryMsg::Balance {
                address: address.to_string(),
            },
        )
        .unwrap();
    bal.balance
}

#[test]
fn deposit_and_withdraw_via_instant_withdraw_cw20() {
    let TestEnv {
        mut app,
        owner,
        user,
        treasury,
        vfdusd,
        ust1,
        window,
        ..
    } = setup();

    app.execute_contract(
        owner.clone(),
        vfdusd.clone(),
        &cw20_mintable::msg::ExecuteMsg::Mint {
            recipient: user.to_string(),
            amount: Uint128::from(10_000_000u128),
        },
        &[],
    )
    .unwrap();

    let dep = Uint128::from(1_000_000u128);
    app.execute_contract(
        user.clone(),
        vfdusd.clone(),
        &Cw20ExecuteMsg::Send {
            contract: window.to_string(),
            amount: dep,
            msg: to_json_binary(&Cw20HookMsg::Deposit {}).unwrap(),
        },
        &[],
    )
    .unwrap();

    assert_eq!(cw20_balance(&app, &vfdusd, &treasury), dep);
    // No CW20 allowance required for redeem.
    let allowance: cw20::AllowanceResponse = app
        .wrap()
        .query_wasm_smart(
            &vfdusd,
            &cw20::Cw20QueryMsg::Allowance {
                owner: treasury.to_string(),
                spender: window.to_string(),
            },
        )
        .unwrap();
    assert_eq!(allowance.allowance, Uint128::zero());

    let ust1_bal = cw20_balance(&app, &ust1, &user);
    assert!(ust1_bal > Uint128::zero());

    app.execute_contract(
        user.clone(),
        ust1.clone(),
        &Cw20ExecuteMsg::Send {
            contract: window.to_string(),
            amount: ust1_bal,
            msg: to_json_binary(&Cw20HookMsg::Withdraw {
                min_vfdusd_out: Uint128::zero(),
            })
            .unwrap(),
        },
        &[],
    )
    .unwrap();

    assert_eq!(cw20_balance(&app, &ust1, &user), Uint128::zero());
    assert!(cw20_balance(&app, &vfdusd, &user) > Uint128::zero());
    assert!(cw20_balance(&app, &vfdusd, &treasury) < dep);
}

#[test]
fn withdraw_msg_is_instant_withdraw_cw20_shape() {
    // Snake_case wire name must match ustr-cmm treasury ExecuteMsg.
    let msg = TreasuryExecuteMsg::InstantWithdrawCw20 {
        recipient: "terra1user".into(),
        token: "terra1vfdusd".into(),
        amount: Uint128::from(42u128),
    };
    let bin = to_json_binary(&msg).unwrap();
    let s = String::from_utf8(bin.to_vec()).unwrap();
    assert!(
        s.contains("instant_withdraw_cw20"),
        "unexpected wire json: {s}"
    );
}

#[test]
fn inv_limit_001_per_tx_exceeded() {
    let TestEnv {
        mut app,
        owner,
        user,
        vfdusd,
        window,
        ..
    } = setup();

    app.execute_contract(
        owner.clone(),
        window.clone(),
        &ExecuteMsg::SetLimits {
            per_tx_ust1_limit: Uint128::from(100u128),
            rolling_24h_ust1_limit: Uint128::from(500_000_000u128),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        owner.clone(),
        vfdusd.clone(),
        &cw20_mintable::msg::ExecuteMsg::Mint {
            recipient: user.to_string(),
            amount: Uint128::from(10_000_000u128),
        },
        &[],
    )
    .unwrap();

    let err = app
        .execute_contract(
            user.clone(),
            vfdusd.clone(),
            &Cw20ExecuteMsg::Send {
                contract: window.to_string(),
                amount: Uint128::from(200_000u128),
                msg: to_json_binary(&Cw20HookMsg::Deposit {}).unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("per-tx"),
        "unexpected: {err}"
    );
}

#[test]
fn inv_limit_001_rolling_24h_exceeded() {
    let TestEnv {
        mut app,
        owner,
        user,
        vfdusd,
        window,
        ..
    } = setup();

    app.execute_contract(
        owner.clone(),
        window.clone(),
        &ExecuteMsg::SetLimits {
            per_tx_ust1_limit: Uint128::from(400_000_000u128),
            rolling_24h_ust1_limit: Uint128::from(500_000_000u128),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        owner.clone(),
        vfdusd.clone(),
        &cw20_mintable::msg::ExecuteMsg::Mint {
            recipient: user.to_string(),
            amount: Uint128::from(1_000_000_000u128),
        },
        &[],
    )
    .unwrap();

    let dep_amt = Uint128::from(300_000_000u128);
    app.execute_contract(
        user.clone(),
        vfdusd.clone(),
        &Cw20ExecuteMsg::Send {
            contract: window.to_string(),
            amount: dep_amt,
            msg: to_json_binary(&Cw20HookMsg::Deposit {}).unwrap(),
        },
        &[],
    )
    .unwrap();

    let err = app
        .execute_contract(
            user.clone(),
            vfdusd,
            &Cw20ExecuteMsg::Send {
                contract: window.to_string(),
                amount: dep_amt,
                msg: to_json_binary(&Cw20HookMsg::Deposit {}).unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("rolling"),
        "unexpected: {err}"
    );
}

#[test]
fn paused_blocks_cw20_flow() {
    let TestEnv {
        mut app,
        owner,
        user,
        vfdusd,
        window,
        ..
    } = setup();

    app.execute_contract(
        owner.clone(),
        window.clone(),
        &ExecuteMsg::SetPaused { paused: true },
        &[],
    )
    .unwrap();

    app.execute_contract(
        owner.clone(),
        vfdusd.clone(),
        &cw20_mintable::msg::ExecuteMsg::Mint {
            recipient: user.to_string(),
            amount: Uint128::from(1_000_000u128),
        },
        &[],
    )
    .unwrap();

    let err = app
        .execute_contract(
            user,
            vfdusd,
            &Cw20ExecuteMsg::Send {
                contract: window.to_string(),
                amount: Uint128::from(100_000u128),
                msg: to_json_binary(&Cw20HookMsg::Deposit {}).unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("paused"),
        "unexpected: {err}"
    );
}

#[test]
fn invalid_cw20_sender_rejected() {
    let TestEnv {
        mut app,
        owner,
        user,
        window,
        ..
    } = setup();

    let cw20_id = app.store_code(cw20_mintable_contract());
    let fake = app
        .instantiate_contract(
            cw20_id,
            owner.clone(),
            &cw20_mintable::msg::InstantiateMsg {
                name: "FAKE".into(),
                symbol: "FAK".into(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: owner.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            &[],
            "fake",
            None,
        )
        .unwrap();

    app.execute_contract(
        owner.clone(),
        fake.clone(),
        &cw20_mintable::msg::ExecuteMsg::Mint {
            recipient: user.to_string(),
            amount: Uint128::from(1_000_000u128),
        },
        &[],
    )
    .unwrap();

    let err = app
        .execute_contract(
            user,
            fake,
            &Cw20ExecuteMsg::Send {
                contract: window.to_string(),
                amount: Uint128::from(100u128),
                msg: to_json_binary(&Cw20HookMsg::Deposit {}).unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("cw20"),
        "unexpected: {err}"
    );
}

#[test]
fn withdraw_below_min_vfdusd_out_rejected() {
    let TestEnv {
        mut app,
        owner,
        user,
        vfdusd,
        ust1,
        window,
        ..
    } = setup();

    app.execute_contract(
        owner.clone(),
        vfdusd.clone(),
        &cw20_mintable::msg::ExecuteMsg::Mint {
            recipient: user.to_string(),
            amount: Uint128::from(10_000_000u128),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        user.clone(),
        vfdusd,
        &Cw20ExecuteMsg::Send {
            contract: window.to_string(),
            amount: Uint128::from(500_000u128),
            msg: to_json_binary(&Cw20HookMsg::Deposit {}).unwrap(),
        },
        &[],
    )
    .unwrap();

    let bal = cw20_balance(&app, &ust1, &user);

    let err = app
        .execute_contract(
            user,
            ust1,
            &Cw20ExecuteMsg::Send {
                contract: window.to_string(),
                amount: bal,
                msg: to_json_binary(&Cw20HookMsg::Withdraw {
                    min_vfdusd_out: Uint128::MAX,
                })
                .unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("minimum"),
        "unexpected: {err}"
    );
}

#[test]
fn withdraw_insufficient_vfdusd_in_treasury() {
    let TestEnv {
        mut app,
        owner,
        user,
        vfdusd,
        ust1,
        window,
        ..
    } = setup();

    app.execute_contract(
        owner.clone(),
        ust1.clone(),
        &cw20_mintable::msg::ExecuteMsg::Mint {
            recipient: user.to_string(),
            amount: Uint128::from(5_000_000u128),
        },
        &[],
    )
    .unwrap();

    let err = app
        .execute_contract(
            user.clone(),
            ust1,
            &Cw20ExecuteMsg::Send {
                contract: window.to_string(),
                amount: Uint128::from(1_000_000u128),
                msg: to_json_binary(&Cw20HookMsg::Withdraw {
                    min_vfdusd_out: Uint128::zero(),
                })
                .unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("vFDUSD"),
        "unexpected: {err}"
    );

    assert_eq!(cw20_balance(&app, &vfdusd, &user), Uint128::zero());
}

#[test]
fn withdraw_treasury_reject_is_atomic_no_ust1_burn() {
    let TestEnv {
        mut app,
        owner,
        user,
        treasury,
        vfdusd,
        ust1,
        window,
        ..
    } = setup_with_treasury(false);

    app.execute_contract(
        owner.clone(),
        vfdusd.clone(),
        &cw20_mintable::msg::ExecuteMsg::Mint {
            recipient: user.to_string(),
            amount: Uint128::from(10_000_000u128),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        user.clone(),
        vfdusd.clone(),
        &Cw20ExecuteMsg::Send {
            contract: window.to_string(),
            amount: Uint128::from(1_000_000u128),
            msg: to_json_binary(&Cw20HookMsg::Deposit {}).unwrap(),
        },
        &[],
    )
    .unwrap();

    let ust1_before = cw20_balance(&app, &ust1, &user);
    let treasury_before = cw20_balance(&app, &vfdusd, &treasury);

    app.execute_contract(
        owner.clone(),
        treasury.clone(),
        &stub_treasury::ExecuteMsg::SetRejectPulls { reject: true },
        &[],
    )
    .unwrap();

    let err = app
        .execute_contract(
            user.clone(),
            ust1.clone(),
            &Cw20ExecuteMsg::Send {
                contract: window.to_string(),
                amount: ust1_before,
                msg: to_json_binary(&Cw20HookMsg::Withdraw {
                    min_vfdusd_out: Uint128::zero(),
                })
                .unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause()
            .to_string()
            .to_lowercase()
            .contains("rejected")
            || err.root_cause().to_string().to_lowercase().contains("unregistered"),
        "unexpected: {err}"
    );

    assert_eq!(cw20_balance(&app, &ust1, &user), ust1_before);
    assert_eq!(cw20_balance(&app, &vfdusd, &treasury), treasury_before);
}

#[test]
fn stale_oracle_blocks_deposit() {
    let TestEnv {
        mut app,
        owner,
        user,
        vfdusd,
        window,
        ..
    } = setup();

    let t = app.block_info().time.seconds();
    app.update_block(|b| {
        b.time = Timestamp::from_seconds(t + DEFAULT_MAX_ORACLE_AGE_SECS + 1);
        b.height += 1;
    });

    app.execute_contract(
        owner.clone(),
        vfdusd.clone(),
        &cw20_mintable::msg::ExecuteMsg::Mint {
            recipient: user.to_string(),
            amount: Uint128::from(1_000_000u128),
        },
        &[],
    )
    .unwrap();

    let err = app
        .execute_contract(
            user,
            vfdusd,
            &Cw20ExecuteMsg::Send {
                contract: window.to_string(),
                amount: Uint128::from(100_000u128),
                msg: to_json_binary(&Cw20HookMsg::Deposit {}).unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("stale"),
        "unexpected: {err}"
    );
}

#[test]
fn set_max_oracle_age_governance_only() {
    let TestEnv {
        mut app,
        owner,
        window,
        ..
    } = setup();

    let stranger = Addr::unchecked("stranger");
    let err = app
        .execute_contract(
            stranger,
            window.clone(),
            &ExecuteMsg::SetMaxOracleAge {
                max_oracle_age_sec: 12 * 60 * 60,
            },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("unauthorized"),
        "unexpected: {err}"
    );

    app.execute_contract(
        owner,
        window.clone(),
        &ExecuteMsg::SetMaxOracleAge {
            max_oracle_age_sec: 12 * 60 * 60,
        },
        &[],
    )
    .unwrap();

    let cfg: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&window, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(cfg.max_oracle_age_sec, 12 * 60 * 60);
}

#[test]
fn set_max_oracle_age_below_oracle_throttle_rejected() {
    let TestEnv {
        mut app,
        owner,
        window,
        ..
    } = setup();

    let err = app
        .execute_contract(
            owner,
            window,
            &ExecuteMsg::SetMaxOracleAge {
                max_oracle_age_sec: MIN_ORACLE_UPDATE_INTERVAL_SECS - 1,
            },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("at least"),
        "unexpected: {err}"
    );
}

#[test]
fn migrate_preserves_config() {
    let TestEnv {
        mut app,
        owner,
        window,
        window_code_id,
        treasury,
        vfdusd,
        ust1,
        ..
    } = setup();

    let before: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&window, &QueryMsg::Config {})
        .unwrap();

    app.migrate_contract(
        owner,
        window.clone(),
        &MigrateMsg {},
        window_code_id,
    )
    .unwrap();

    let after: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&window, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(after.cmm_treasury, treasury.to_string());
    assert_eq!(after.vfdusd_token, vfdusd.to_string());
    assert_eq!(after.ust1_token, ust1.to_string());
}
