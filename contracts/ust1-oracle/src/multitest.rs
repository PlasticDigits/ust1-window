//! cw-multi-test coverage for **INV-ORACLE-THROTTLE-001**, **INV-ORACLE-DAILY-001**,
//! **INV-ORACLE-MONO-001**, pause/ACL, and governance flows.

use cosmwasm_std::{Addr, Empty, Timestamp, Uint128};
use cw_multi_test::{App, ContractWrapper, Executor};

use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use ust1_common::math::max_rate_after_daily_cap;
use ust1_common::RATE_SCALE;

fn oracle_contract() -> Box<dyn cw_multi_test::Contract<Empty>> {
    let c = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_migrate(crate::contract::migrate);
    Box::new(c)
}

fn set_time(app: &mut App, secs: u64) {
    app.update_block(|b| {
        b.time = Timestamp::from_seconds(secs);
        b.height += 1;
    });
}

#[test]
fn inv_oracle_mono_update_success_within_cap() {
    // **INV-ORACLE-MONO-001** / **INV-ORACLE-DAILY-001**: monotonic bump within +2% UTC cap.
    let mut app = App::default();
    let gov = Addr::unchecked("gov");
    let bot = Addr::unchecked("bot");
    let code = app.store_code(oracle_contract());
    let initial = Uint128::from(RATE_SCALE);
    let oracle = app
        .instantiate_contract(
            code,
            gov.clone(),
            &InstantiateMsg {
                governance: gov.to_string(),
                oracle_operator: bot.to_string(),
                initial_rate: initial,
            },
            &[],
            "oracle",
            None,
        )
        .unwrap();

    set_time(&mut app, 86_400 + 500);
    let bump = initial
        .checked_mul(Uint128::from(101u128))
        .unwrap()
        .checked_div(Uint128::from(100u128))
        .unwrap();

    app.execute_contract(
        bot.clone(),
        oracle.clone(),
        &ExecuteMsg::UpdateRate { new_rate: bump },
        &[],
    )
    .unwrap();

    let st: crate::msg::StateResponse = app
        .wrap()
        .query_wasm_smart(oracle, &QueryMsg::State {})
        .unwrap();
    assert_eq!(st.rate, bump);
    assert!(!st.paused);
}

#[test]
fn state_surfaces_paused_flag_for_circuit_breaker() {
    // **INV-ORACLE-PAUSE-001**: State.paused mirrors Config so window readers fail closed.
    let mut app = App::default();
    let gov = Addr::unchecked("gov");
    let bot = Addr::unchecked("bot");
    let code = app.store_code(oracle_contract());
    let initial = Uint128::from(RATE_SCALE);
    let oracle = app
        .instantiate_contract(
            code,
            gov.clone(),
            &InstantiateMsg {
                governance: gov.to_string(),
                oracle_operator: bot.to_string(),
                initial_rate: initial,
            },
            &[],
            "oracle",
            None,
        )
        .unwrap();

    let st0: crate::msg::StateResponse = app
        .wrap()
        .query_wasm_smart(&oracle, &QueryMsg::State {})
        .unwrap();
    assert!(!st0.paused);

    app.execute_contract(
        gov.clone(),
        oracle.clone(),
        &ExecuteMsg::SetPaused { paused: true },
        &[],
    )
    .unwrap();

    let st1: crate::msg::StateResponse = app
        .wrap()
        .query_wasm_smart(&oracle, &QueryMsg::State {})
        .unwrap();
    assert!(st1.paused);

    let stranger = Addr::unchecked("stranger");
    let err = app
        .execute_contract(
            stranger,
            oracle.clone(),
            &ExecuteMsg::SetPaused { paused: false },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("unauthorized"),
        "unexpected: {err}"
    );

    // Operator may UpdateRate when unpaused, but must never trip/clear the breaker.
    let err_bot = app
        .execute_contract(
            bot,
            oracle.clone(),
            &ExecuteMsg::SetPaused { paused: false },
            &[],
        )
        .unwrap_err();
    assert!(
        err_bot.root_cause().to_string().contains("unauthorized"),
        "operator must not SetPaused: {err_bot}"
    );

    app.execute_contract(
        gov,
        oracle.clone(),
        &ExecuteMsg::SetPaused { paused: false },
        &[],
    )
    .unwrap();
    let st2: crate::msg::StateResponse = app
        .wrap()
        .query_wasm_smart(oracle, &QueryMsg::State {})
        .unwrap();
    assert!(!st2.paused);
}

#[test]
fn inv_oracle_throttle_second_update_too_soon() {
    // **INV-ORACLE-THROTTLE-001**
    let mut app = App::default();
    let gov = Addr::unchecked("gov");
    let bot = Addr::unchecked("bot");
    let code = app.store_code(oracle_contract());
    let initial = Uint128::from(RATE_SCALE);
    let oracle = app
        .instantiate_contract(
            code,
            gov,
            &InstantiateMsg {
                governance: "gov".into(),
                oracle_operator: bot.to_string(),
                initial_rate: initial,
            },
            &[],
            "oracle",
            None,
        )
        .unwrap();

    set_time(&mut app, 10_000);
    app.execute_contract(
        bot.clone(),
        oracle.clone(),
        &ExecuteMsg::UpdateRate {
            new_rate: initial
                .checked_mul(Uint128::from(101u128))
                .unwrap()
                .checked_div(Uint128::from(100u128))
                .unwrap(),
        },
        &[],
    )
    .unwrap();

    let err = app
        .execute_contract(
            bot.clone(),
            oracle.clone(),
            &ExecuteMsg::UpdateRate { new_rate: initial },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("too soon"),
        "unexpected: {err}"
    );
}

#[test]
fn inv_oracle_mono_decrease_rejected() {
    // **INV-ORACLE-MONO-001**
    let mut app = App::default();
    let gov = Addr::unchecked("gov");
    let bot = Addr::unchecked("bot");
    let code = app.store_code(oracle_contract());
    let initial = Uint128::from(RATE_SCALE);
    let oracle = app
        .instantiate_contract(
            code,
            gov,
            &InstantiateMsg {
                governance: "gov".into(),
                oracle_operator: bot.to_string(),
                initial_rate: initial,
            },
            &[],
            "oracle",
            None,
        )
        .unwrap();

    set_time(&mut app, 10_000);
    let higher = initial
        .checked_mul(Uint128::from(102u128))
        .unwrap()
        .checked_div(Uint128::from(100u128))
        .unwrap();
    app.execute_contract(
        bot.clone(),
        oracle.clone(),
        &ExecuteMsg::UpdateRate { new_rate: higher },
        &[],
    )
    .unwrap();

    set_time(
        &mut app,
        10_000 + ust1_common::MIN_ORACLE_UPDATE_INTERVAL_SECS,
    );
    let err = app
        .execute_contract(
            bot,
            oracle,
            &ExecuteMsg::UpdateRate { new_rate: initial },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("decrease"),
        "unexpected: {err}"
    );
}

#[test]
fn inv_oracle_daily_cap_exceeded_same_utc_day() {
    // **INV-ORACLE-DAILY-001**
    let mut app = App::default();
    let gov = Addr::unchecked("gov");
    let bot = Addr::unchecked("bot");
    let code = app.store_code(oracle_contract());
    let initial = Uint128::from(RATE_SCALE);
    let oracle = app
        .instantiate_contract(
            code,
            gov,
            &InstantiateMsg {
                governance: "gov".into(),
                oracle_operator: bot.to_string(),
                initial_rate: initial,
            },
            &[],
            "oracle",
            None,
        )
        .unwrap();

    let day_start = 86_400u64 * 10;
    set_time(&mut app, day_start + 100);
    // Bootstrap first update (last_update_sec==0 skips daily cap) so later caps apply.
    app.execute_contract(
        bot.clone(),
        oracle.clone(),
        &ExecuteMsg::UpdateRate {
            new_rate: initial,
        },
        &[],
    )
    .unwrap();
    set_time(
        &mut app,
        day_start + 100 + ust1_common::MIN_ORACLE_UPDATE_INTERVAL_SECS + 1,
    );
    let max_r = max_rate_after_daily_cap(initial).unwrap();
    let too_high = max_r.checked_add(Uint128::one()).unwrap();

    let err = app
        .execute_contract(
            bot,
            oracle,
            &ExecuteMsg::UpdateRate { new_rate: too_high },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("daily"),
        "unexpected: {err}"
    );
}

#[test]
fn inv_oracle_pause_blocks_operator_update() {
    let mut app = App::default();
    let gov = Addr::unchecked("gov");
    let bot = Addr::unchecked("bot");
    let code = app.store_code(oracle_contract());
    let initial = Uint128::from(RATE_SCALE);
    let oracle = app
        .instantiate_contract(
            code,
            gov.clone(),
            &InstantiateMsg {
                governance: gov.to_string(),
                oracle_operator: bot.to_string(),
                initial_rate: initial,
            },
            &[],
            "oracle",
            None,
        )
        .unwrap();

    app.execute_contract(
        gov.clone(),
        oracle.clone(),
        &ExecuteMsg::SetPaused { paused: true },
        &[],
    )
    .unwrap();

    set_time(&mut app, 50_000);
    let err = app
        .execute_contract(
            bot,
            oracle,
            &ExecuteMsg::UpdateRate {
                new_rate: initial
                    .checked_mul(Uint128::from(101u128))
                    .unwrap()
                    .checked_div(Uint128::from(100u128))
                    .unwrap(),
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
fn inv_oracle_unauthorized_update_not_operator() {
    let mut app = App::default();
    let gov = Addr::unchecked("gov");
    let bot = Addr::unchecked("bot");
    let code = app.store_code(oracle_contract());
    let initial = Uint128::from(RATE_SCALE);
    let oracle = app
        .instantiate_contract(
            code,
            gov,
            &InstantiateMsg {
                governance: "gov".into(),
                oracle_operator: bot.to_string(),
                initial_rate: initial,
            },
            &[],
            "oracle",
            None,
        )
        .unwrap();

    set_time(&mut app, 12_000);
    let stranger = Addr::unchecked("stranger");
    let err = app
        .execute_contract(
            stranger,
            oracle,
            &ExecuteMsg::UpdateRate { new_rate: initial },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("unauthorized"),
        "unexpected: {err}"
    );
}

#[test]
fn governance_rotates_oracle_operator() {
    let mut app = App::default();
    let gov = Addr::unchecked("gov");
    let bot = Addr::unchecked("bot");
    let bot2 = Addr::unchecked("bot2");
    let code = app.store_code(oracle_contract());
    let initial = Uint128::from(RATE_SCALE);
    let oracle = app
        .instantiate_contract(
            code,
            gov.clone(),
            &InstantiateMsg {
                governance: gov.to_string(),
                oracle_operator: bot.to_string(),
                initial_rate: initial,
            },
            &[],
            "oracle",
            None,
        )
        .unwrap();

    app.execute_contract(
        gov.clone(),
        oracle.clone(),
        &ExecuteMsg::SetOracleOperator {
            address: bot2.to_string(),
        },
        &[],
    )
    .unwrap();

    set_time(&mut app, 20_000);
    let err = app
        .execute_contract(
            bot.clone(),
            oracle.clone(),
            &ExecuteMsg::UpdateRate {
                new_rate: initial
                    .checked_mul(Uint128::from(101u128))
                    .unwrap()
                    .checked_div(Uint128::from(100u128))
                    .unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("unauthorized"),
        "unexpected: {err}"
    );

    app.execute_contract(
        bot2,
        oracle,
        &ExecuteMsg::UpdateRate {
            new_rate: initial
                .checked_mul(Uint128::from(101u128))
                .unwrap()
                .checked_div(Uint128::from(100u128))
                .unwrap(),
        },
        &[],
    )
    .unwrap();
}

#[test]
fn governance_two_step_transfer() {
    let mut app = App::default();
    let gov = Addr::unchecked("gov");
    let bot = Addr::unchecked("bot");
    let next_gov = Addr::unchecked("next_gov");
    let code = app.store_code(oracle_contract());
    let initial = Uint128::from(RATE_SCALE);
    let oracle = app
        .instantiate_contract(
            code,
            gov.clone(),
            &InstantiateMsg {
                governance: gov.to_string(),
                oracle_operator: bot.to_string(),
                initial_rate: initial,
            },
            &[],
            "oracle",
            None,
        )
        .unwrap();

    app.execute_contract(
        gov.clone(),
        oracle.clone(),
        &ExecuteMsg::ProposeGovernance {
            address: next_gov.to_string(),
        },
        &[],
    )
    .unwrap();

    let err = app
        .execute_contract(bot, oracle.clone(), &ExecuteMsg::AcceptGovernance {}, &[])
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("governance"),
        "unexpected: {err}"
    );

    app.execute_contract(
        next_gov.clone(),
        oracle.clone(),
        &ExecuteMsg::AcceptGovernance {},
        &[],
    )
    .unwrap();

    let cfg: crate::msg::ConfigResponse = app
        .wrap()
        .query_wasm_smart(&oracle, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(cfg.governance, next_gov.to_string());

    let err = app
        .execute_contract(
            gov.clone(),
            oracle,
            &ExecuteMsg::SetPaused { paused: true },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("unauthorized"),
        "unexpected: {err}"
    );
}
