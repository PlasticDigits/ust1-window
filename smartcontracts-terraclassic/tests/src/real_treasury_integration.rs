//! Integration against the **real** ustr-cmm `treasury` crate (pinned git rev).
//!
//! This is the CI-gated equivalent of loading `treasury.wasm` from that rev: same
//! `ExecuteMsg` decode, spender ACL, fail-closed 24h limit, and CW20 Transfer path.
//! Fast stub coverage remains in `integration_tests.rs` / `stub_treasury.rs`.
//!
//! Skip offline only if the git dependency cannot be resolved (normal `cargo test`
//! fetches the pin). See issue #21 / INV-SCHEMA-001.

use cosmwasm_std::{to_json_binary, Addr, Empty, Uint128};
use cw20::{Cw20ExecuteMsg, MinterResponse};
use cw_multi_test::{App, ContractWrapper, Executor};
use ust1_common::{
    DEFAULT_FEE_BPS, DEFAULT_PER_TX_UST1_LIMIT, DEFAULT_ROLLING_24H_UST1_LIMIT, RATE_SCALE,
};
use ust1_oracle::msg as oracle_msg;
use ust1_window::msg as window_msg;
use ust1_window::treasury::USTR_CMM_TREASURY_SCHEMA_REV;

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
        ust1_window::contract::execute,
        ust1_window::contract::instantiate,
        ust1_window::contract::query,
    )
    .with_migrate(ust1_window::contract::migrate);
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

fn real_treasury_contract() -> Box<dyn cw_multi_test::Contract<Empty>> {
    // Real ustr-cmm treasury entry points (library feature — not a local stub).
    let c = ContractWrapper::new(
        cmm_treasury::contract::execute,
        cmm_treasury::contract::instantiate,
        cmm_treasury::contract::query,
    )
    .with_migrate(cmm_treasury::contract::migrate);
    Box::new(c)
}

struct Env {
    app: App,
    owner: Addr,
    bot: Addr,
    user: Addr,
    treasury: Addr,
    oracle: Addr,
    vfdusd: Addr,
    ust1: Addr,
    window: Addr,
}

fn setup(register_spender: bool) -> Env {
    let mut app = App::default();
    let owner = Addr::unchecked("owner");
    let bot = Addr::unchecked("bot");
    let user = Addr::unchecked("user");

    let oracle_id = app.store_code(oracle_contract());
    let window_id = app.store_code(window_contract());
    let cw20_id = app.store_code(cw20_mintable_contract());
    let treasury_id = app.store_code(real_treasury_contract());

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
            &cmm_treasury::msg::InstantiateMsg {
                governance: owner.to_string(),
            },
            &[],
            "treasury",
            None,
        )
        .unwrap();

    let window = app
        .instantiate_contract(
            window_id,
            owner.clone(),
            &window_msg::InstantiateMsg {
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
            None,
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

    if register_spender {
        app.execute_contract(
            owner.clone(),
            treasury.clone(),
            &cmm_treasury::msg::ExecuteMsg::SetCw20Spender {
                token: vfdusd.to_string(),
                spender: window.to_string(),
                limit_24h: Some(Uint128::from(10_000_000_000u128)),
            },
            &[],
        )
        .unwrap();
    }

    // Seed oracle freshness.
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

    Env {
        app,
        owner,
        bot,
        user,
        treasury,
        oracle,
        vfdusd,
        ust1,
        window,
    }
}

fn cw20_balance(app: &App, token: &Addr, addr: &Addr) -> Uint128 {
    let bal: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            token,
            &cw20::Cw20QueryMsg::Balance {
                address: addr.to_string(),
            },
        )
        .unwrap();
    bal.balance
}

fn deposit(env: &mut Env, amount: Uint128) {
    env.app
        .execute_contract(
            env.owner.clone(),
            env.vfdusd.clone(),
            &cw20_mintable::msg::ExecuteMsg::Mint {
                recipient: env.user.to_string(),
                amount,
            },
            &[],
        )
        .unwrap();
    env.app
        .execute_contract(
            env.user.clone(),
            env.vfdusd.clone(),
            &Cw20ExecuteMsg::Send {
                contract: env.window.to_string(),
                amount,
                msg: to_json_binary(&window_msg::Cw20HookMsg::Deposit {}).unwrap(),
            },
            &[],
        )
        .unwrap();
}

#[test]
fn real_treasury_pin_is_schema_rev() {
    // Compile-time reminder: workspace `cmm-treasury` rev must match this constant.
    assert_eq!(
        USTR_CMM_TREASURY_SCHEMA_REV,
        "e6c4b7cf33f2f56d21c0e9fb2828efe87f032ded"
    );
}

#[test]
fn real_treasury_registered_spender_withdraw_succeeds() {
    let mut env = setup(true);
    let dep = Uint128::from(1_000_000u128);
    deposit(&mut env, dep);

    assert_eq!(cw20_balance(&env.app, &env.vfdusd, &env.treasury), dep);
    let ust1_bal = cw20_balance(&env.app, &env.ust1, &env.user);
    assert!(ust1_bal > Uint128::zero());

    env.app
        .execute_contract(
            env.user.clone(),
            env.ust1.clone(),
            &Cw20ExecuteMsg::Send {
                contract: env.window.to_string(),
                amount: ust1_bal,
                msg: to_json_binary(&window_msg::Cw20HookMsg::Withdraw {
                    min_vfdusd_out: Uint128::zero(),
                })
                .unwrap(),
            },
            &[],
        )
        .unwrap();

    assert_eq!(
        cw20_balance(&env.app, &env.ust1, &env.user),
        Uint128::zero()
    );
    assert!(cw20_balance(&env.app, &env.vfdusd, &env.user) > Uint128::zero());
    assert!(cw20_balance(&env.app, &env.vfdusd, &env.treasury) < dep);

    // No CW20 allowance involved.
    let allowance: cw20::AllowanceResponse = env
        .app
        .wrap()
        .query_wasm_smart(
            env.vfdusd.clone(),
            &cw20::Cw20QueryMsg::Allowance {
                owner: env.treasury.to_string(),
                spender: env.window.to_string(),
            },
        )
        .unwrap();
    assert_eq!(allowance.allowance, Uint128::zero());
}

#[test]
fn real_treasury_unregistered_spender_reverts_atomically() {
    let mut env = setup(false);
    let dep = Uint128::from(1_000_000u128);
    deposit(&mut env, dep);
    let ust1_before = cw20_balance(&env.app, &env.ust1, &env.user);
    let treasury_before = cw20_balance(&env.app, &env.vfdusd, &env.treasury);

    let err = env
        .app
        .execute_contract(
            env.user.clone(),
            env.ust1.clone(),
            &Cw20ExecuteMsg::Send {
                contract: env.window.to_string(),
                amount: ust1_before,
                msg: to_json_binary(&window_msg::Cw20HookMsg::Withdraw {
                    min_vfdusd_out: Uint128::zero(),
                })
                .unwrap(),
            },
            &[],
        )
        .unwrap_err();
    let msg = err.root_cause().to_string();
    assert!(
        msg.contains("NoCw20Spender")
            || msg.contains("spender")
            || msg.contains("not registered")
            || msg.contains("NotRegistered"),
        "unexpected err: {msg}"
    );

    // Atomic: UST1 not burned, treasury inventory unchanged.
    assert_eq!(cw20_balance(&env.app, &env.ust1, &env.user), ust1_before);
    assert_eq!(
        cw20_balance(&env.app, &env.vfdusd, &env.treasury),
        treasury_before
    );
}

#[test]
fn deposit_still_forwards_vfdusd_to_real_treasury() {
    let mut env = setup(true);
    let dep = Uint128::from(2_500_000u128);
    deposit(&mut env, dep);
    assert_eq!(cw20_balance(&env.app, &env.vfdusd, &env.treasury), dep);
    assert_eq!(
        cw20_balance(&env.app, &env.vfdusd, &env.window),
        Uint128::zero()
    );
    let _ = (&env.bot, &env.oracle);
}
