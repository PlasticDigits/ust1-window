//! cw-multi-test coverage for **INV-LIMIT-001**, **INV-SWAP-001**, pause/ACL, and failure paths.

use cosmwasm_std::{to_json_binary, Addr, Empty, Timestamp, Uint128};
use cw20::{Cw20ExecuteMsg, MinterResponse};
use cw_multi_test::{App, ContractWrapper, Executor};

use crate::msg::{Cw20HookMsg, ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use ust1_common::{
    DEFAULT_MAX_ORACLE_AGE_SECS, MIN_ORACLE_UPDATE_INTERVAL_SECS, RATE_SCALE,
};
use ust1_oracle::msg as oracle_msg;

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

struct Env {
    app: App,
    owner: Addr,
    user: Addr,
    vfdusd: Addr,
    ust1: Addr,
    window: Addr,
}

fn setup() -> Env {
    let mut app = App::default();
    let owner = Addr::unchecked("owner");
    let bot = Addr::unchecked("bot");
    let user = Addr::unchecked("user");

    let oracle_id = app.store_code(oracle_contract());
    let window_id = app.store_code(window_contract());
    let cw20_id = app.store_code(cw20_mintable_contract());

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

    let window = app
        .instantiate_contract(
            window_id,
            owner.clone(),
            &InstantiateMsg {
                governance: owner.to_string(),
                oracle: oracle.to_string(),
                vfdusd_token: vfdusd.to_string(),
                ust1_token: ust1.to_string(),
                fee_bps: 50,
                per_tx_ust1_limit: Uint128::from(500_000_000u128),
                rolling_24h_ust1_limit: Uint128::from(2_500_000_000u128),
                max_oracle_age_sec: None,
            },
            &[],
            "window",
            None,
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

    Env {
        app,
        owner,
        user,
        vfdusd,
        ust1,
        window,
    }
}

#[test]
fn inv_limit_001_per_tx_exceeded() {
    let Env {
        mut app,
        owner,
        user,
        vfdusd,
        ust1: _ust1,
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
    let Env {
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
    let Env {
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
    let Env {
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
    let Env {
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

    let bal: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            ust1.clone(),
            &cw20::Cw20QueryMsg::Balance {
                address: user.to_string(),
            },
        )
        .unwrap();

    let err = app
        .execute_contract(
            user,
            ust1,
            &Cw20ExecuteMsg::Send {
                contract: window.to_string(),
                amount: bal.balance,
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
fn withdraw_insufficient_vfdusd_in_window() {
    let Env {
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

    let vb: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            vfdusd,
            &cw20::Cw20QueryMsg::Balance {
                address: user.to_string(),
            },
        )
        .unwrap();
    assert_eq!(vb.balance, Uint128::zero());
}

#[test]
fn stale_oracle_blocks_deposit() {
    let Env {
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
    let Env {
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
    let Env {
        mut app, owner, window, ..
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
