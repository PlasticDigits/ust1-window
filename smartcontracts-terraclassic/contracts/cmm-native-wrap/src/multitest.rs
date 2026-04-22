//! cw-multi-test: **INV-LIMIT-NATIVE-001**, wrap/unwrap, pause, ACL.

use cosmwasm_std::{coin, to_json_binary, Addr, Empty, Timestamp, Uint128};
use cw20::{Cw20ExecuteMsg, MinterResponse};
use cw_multi_test::{App, ContractWrapper, Executor};

use crate::msg::{Cw20HookMsg, ExecuteMsg, InstantiateMsg, PairInstantiateMsg};
use crate::state::{LUNC_DENOM, USTC_DENOM};

fn wrap_contract() -> Box<dyn cw_multi_test::Contract<Empty>> {
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
    wlunc: Addr,
    wustc: Addr,
    wrap: Addr,
}

fn setup() -> Env {
    let mut app = App::default();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user");

    app.init_modules(|router, _api, storage| {
        router
            .bank
            .init_balance(
                storage,
                &user,
                vec![
                    coin(1_000_000_000u128, LUNC_DENOM),
                    coin(1_000_000_000u128, USTC_DENOM),
                ],
            )
            .unwrap();
    });

    let wrap_id = app.store_code(wrap_contract());
    let cw20_id = app.store_code(cw20_mintable_contract());

    let wlunc = app
        .instantiate_contract(
            cw20_id,
            owner.clone(),
            &cw20_mintable::msg::InstantiateMsg {
                name: "wLUNC".into(),
                symbol: "wLUNC".into(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: owner.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            &[],
            "wlunc",
            None,
        )
        .unwrap();

    let wustc = app
        .instantiate_contract(
            cw20_id,
            owner.clone(),
            &cw20_mintable::msg::InstantiateMsg {
                name: "wUSTC".into(),
                symbol: "wUSTC".into(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: owner.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            &[],
            "wustc",
            None,
        )
        .unwrap();

    let wrap = app
        .instantiate_contract(
            wrap_id,
            owner.clone(),
            &InstantiateMsg {
                governance: owner.to_string(),
                fee_bps: 50,
                pairs: vec![
                    PairInstantiateMsg {
                        native_denom: LUNC_DENOM.to_string(),
                        wrapped_token: wlunc.to_string(),
                        per_tx_wrap_limit: Uint128::from(500_000_000u128),
                        rolling_24h_wrap_limit: Uint128::from(2_500_000_000u128),
                    },
                    PairInstantiateMsg {
                        native_denom: USTC_DENOM.to_string(),
                        wrapped_token: wustc.to_string(),
                        per_tx_wrap_limit: Uint128::from(500_000_000u128),
                        rolling_24h_wrap_limit: Uint128::from(2_500_000_000u128),
                    },
                ],
            },
            &[],
            "wrap",
            None,
        )
        .unwrap();

    for t in [&wlunc, &wustc] {
        app.execute_contract(
            owner.clone(),
            t.clone(),
            &cw20_mintable::msg::ExecuteMsg::AddMinter {
                minter: wrap.to_string(),
            },
            &[],
        )
        .unwrap();
    }

    Env {
        app,
        owner,
        user,
        wlunc,
        wustc,
        wrap,
    }
}

#[test]
fn wrap_lunc_mints_wlunc_after_fee() {
    let Env {
        mut app,
        user,
        wlunc,
        wrap,
        ..
    } = setup();

    let in_amt = 1_000_000u128;
    app.execute_contract(
        user.clone(),
        wrap.clone(),
        &ExecuteMsg::Wrap {},
        &[coin(in_amt, LUNC_DENOM)],
    )
    .unwrap();

    let bal: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            wlunc,
            &cw20::Cw20QueryMsg::Balance {
                address: user.to_string(),
            },
        )
        .unwrap();
    // 50 bps on 1_000_000 → 995_000
    assert_eq!(bal.balance, Uint128::from(995_000u128));
}

#[test]
fn unwrap_wustc_returns_uusd() {
    let Env {
        mut app,
        user,
        wustc,
        wrap,
        ..
    } = setup();

    let in_amt = 2_000_000u128;
    app.execute_contract(
        user.clone(),
        wrap.clone(),
        &ExecuteMsg::Wrap {},
        &[coin(in_amt, USTC_DENOM)],
    )
    .unwrap();

    app.execute_contract(
        user.clone(),
        wustc.clone(),
        &Cw20ExecuteMsg::Send {
            contract: wrap.to_string(),
            amount: Uint128::from(1_000_000u128),
            msg: to_json_binary(&Cw20HookMsg::Unwrap {
                min_native_out: Uint128::zero(),
            })
            .unwrap(),
        },
        &[],
    )
    .unwrap();

    let native = app
        .wrap()
        .query_balance(user.to_string(), USTC_DENOM)
        .unwrap();
    // 1B − 2M (wrap) + 995_000 (unwrap out after 50 bps) = 998_995_000
    assert_eq!(native.amount.u128(), 998_995_000u128);
}

#[test]
fn per_tx_limit_blocks_wrap() {
    let Env {
        mut app,
        owner,
        user,
        wrap,
        ..
    } = setup();

    app.execute_contract(
        owner.clone(),
        wrap.clone(),
        &ExecuteMsg::SetPairLimits {
            native_denom: LUNC_DENOM.to_string(),
            per_tx_wrap_limit: Uint128::from(100u128),
            rolling_24h_wrap_limit: Uint128::from(500_000_000u128),
        },
        &[],
    )
    .unwrap();

    let err = app
        .execute_contract(
            user,
            wrap,
            &ExecuteMsg::Wrap {},
            &[coin(1_000_000u128, LUNC_DENOM)],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("per-tx"),
        "unexpected: {err}"
    );
}

#[test]
fn rolling_limit_blocks_second_wrap() {
    let Env {
        mut app,
        owner,
        user,
        wrap,
        ..
    } = setup();

    app.execute_contract(
        owner.clone(),
        wrap.clone(),
        &ExecuteMsg::SetPairLimits {
            native_denom: LUNC_DENOM.to_string(),
            per_tx_wrap_limit: Uint128::from(400_000_000u128),
            rolling_24h_wrap_limit: Uint128::from(500_000_000u128),
        },
        &[],
    )
    .unwrap();

    let dep = 300_000_000u128;
    app.execute_contract(
        user.clone(),
        wrap.clone(),
        &ExecuteMsg::Wrap {},
        &[coin(dep, LUNC_DENOM)],
    )
    .unwrap();

    let err = app
        .execute_contract(user, wrap, &ExecuteMsg::Wrap {}, &[coin(dep, LUNC_DENOM)])
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("rolling"),
        "unexpected: {err}"
    );
}

#[test]
fn paused_blocks_wrap() {
    let Env {
        mut app,
        owner,
        user,
        wrap,
        ..
    } = setup();

    app.execute_contract(
        owner,
        wrap.clone(),
        &ExecuteMsg::SetPaused { paused: true },
        &[],
    )
    .unwrap();

    let err = app
        .execute_contract(
            user,
            wrap,
            &ExecuteMsg::Wrap {},
            &[coin(100_000u128, LUNC_DENOM)],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("paused"),
        "unexpected: {err}"
    );
}

#[test]
fn unknown_cw20_rejected() {
    let Env {
        mut app,
        owner,
        user,
        wrap,
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
        owner,
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
                contract: wrap.to_string(),
                amount: Uint128::from(100u128),
                msg: to_json_binary(&Cw20HookMsg::Unwrap {
                    min_native_out: Uint128::zero(),
                })
                .unwrap(),
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
fn unwrap_below_min_native_rejected() {
    let Env {
        mut app,
        user,
        wlunc,
        wrap,
        ..
    } = setup();

    app.execute_contract(
        user.clone(),
        wrap.clone(),
        &ExecuteMsg::Wrap {},
        &[coin(5_000_000u128, LUNC_DENOM)],
    )
    .unwrap();

    let bal: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            wlunc.clone(),
            &cw20::Cw20QueryMsg::Balance {
                address: user.to_string(),
            },
        )
        .unwrap();

    let err = app
        .execute_contract(
            user,
            wlunc,
            &Cw20ExecuteMsg::Send {
                contract: wrap.to_string(),
                amount: bal.balance,
                msg: to_json_binary(&Cw20HookMsg::Unwrap {
                    min_native_out: Uint128::MAX,
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
fn unwrap_insufficient_native_in_contract() {
    let Env {
        mut app,
        owner,
        user,
        wlunc,
        wrap,
        ..
    } = setup();

    app.execute_contract(
        owner.clone(),
        wlunc.clone(),
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
            wlunc,
            &Cw20ExecuteMsg::Send {
                contract: wrap.to_string(),
                amount: Uint128::from(1_000_000u128),
                msg: to_json_binary(&Cw20HookMsg::Unwrap {
                    min_native_out: Uint128::zero(),
                })
                .unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("native"),
        "unexpected: {err}"
    );

    let native = app
        .wrap()
        .query_balance(user.to_string(), LUNC_DENOM)
        .unwrap();
    assert_eq!(native.amount.u128(), 1_000_000_000u128);
}

#[test]
fn two_native_coins_rejected() {
    let Env {
        mut app,
        user,
        wrap,
        ..
    } = setup();

    let err = app
        .execute_contract(
            user,
            wrap,
            &ExecuteMsg::Wrap {},
            &[coin(100u128, LUNC_DENOM), coin(100u128, USTC_DENOM)],
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("native")
            || err.root_cause().to_string().contains("funds"),
        "unexpected: {err}"
    );
}

#[test]
fn instantiate_wrong_denom_set_rejected() {
    let mut app = App::default();
    let owner = Addr::unchecked("owner");
    let wrap_id = app.store_code(wrap_contract());
    let cw20_id = app.store_code(cw20_mintable_contract());

    let a = app
        .instantiate_contract(
            cw20_id,
            owner.clone(),
            &cw20_mintable::msg::InstantiateMsg {
                name: "TokA".into(),
                symbol: "TKA".into(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: owner.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            &[],
            "a",
            None,
        )
        .unwrap();
    let b = app
        .instantiate_contract(
            cw20_id,
            owner.clone(),
            &cw20_mintable::msg::InstantiateMsg {
                name: "TokB".into(),
                symbol: "TKB".into(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: owner.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            &[],
            "b",
            None,
        )
        .unwrap();

    let err = app
        .instantiate_contract(
            wrap_id,
            owner,
            &InstantiateMsg {
                governance: Addr::unchecked("gov").to_string(),
                fee_bps: 0,
                pairs: vec![
                    PairInstantiateMsg {
                        native_denom: "uatom".into(),
                        wrapped_token: a.to_string(),
                        per_tx_wrap_limit: Uint128::MAX,
                        rolling_24h_wrap_limit: Uint128::MAX,
                    },
                    PairInstantiateMsg {
                        native_denom: "uosmo".into(),
                        wrapped_token: b.to_string(),
                        per_tx_wrap_limit: Uint128::MAX,
                        rolling_24h_wrap_limit: Uint128::MAX,
                    },
                ],
            },
            &[],
            "bad",
            None,
        )
        .unwrap_err();
    assert!(
        err.root_cause().to_string().contains("uluna")
            || err.root_cause().to_string().contains("instantiate"),
        "unexpected: {err}"
    );
}

#[test]
fn rolling_window_resets_after_24h() {
    let Env {
        mut app,
        owner,
        user,
        wrap,
        ..
    } = setup();

    app.execute_contract(
        owner,
        wrap.clone(),
        &ExecuteMsg::SetPairLimits {
            native_denom: LUNC_DENOM.to_string(),
            per_tx_wrap_limit: Uint128::from(400_000_000u128),
            rolling_24h_wrap_limit: Uint128::from(500_000_000u128),
        },
        &[],
    )
    .unwrap();

    let dep = 300_000_000u128;
    app.execute_contract(
        user.clone(),
        wrap.clone(),
        &ExecuteMsg::Wrap {},
        &[coin(dep, LUNC_DENOM)],
    )
    .unwrap();

    let t = app.block_info().time.seconds();
    app.update_block(|b| {
        b.time = Timestamp::from_seconds(t + 86_400 + 1);
        b.height += 1;
    });

    app.execute_contract(user, wrap, &ExecuteMsg::Wrap {}, &[coin(dep, LUNC_DENOM)])
        .unwrap();
}
