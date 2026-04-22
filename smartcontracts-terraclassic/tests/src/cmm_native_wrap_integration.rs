//! Workspace integration: `cmm-native-wrap` queries and wrap path.

use cosmwasm_std::{coin, Addr, Empty, Uint128};
use cw20::MinterResponse;
use cw_multi_test::{App, ContractWrapper, Executor};

use cmm_native_wrap::msg::{ExecuteMsg, InstantiateMsg, PairInstantiateMsg, QueryMsg};
use cmm_native_wrap::state::{LUNC_DENOM, USTC_DENOM};
use ust1_common::DEFAULT_FEE_BPS;

fn wrap_contract() -> Box<dyn cw_multi_test::Contract<Empty>> {
    Box::new(
        ContractWrapper::new(
            cmm_native_wrap::contract::execute,
            cmm_native_wrap::contract::instantiate,
            cmm_native_wrap::contract::query,
        )
        .with_migrate(cmm_native_wrap::contract::migrate),
    )
}

fn cw20_mintable_contract() -> Box<dyn cw_multi_test::Contract<Empty>> {
    Box::new(ContractWrapper::new(
        cw20_mintable::contract::execute,
        cw20_mintable::contract::instantiate,
        cw20_mintable::contract::query,
    ))
}

#[test]
fn config_and_effective_wrap_query() {
    let mut app = App::default();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user");

    app.init_modules(|router, _api, storage| {
        router
            .bank
            .init_balance(storage, &user, vec![coin(10_000_000u128, LUNC_DENOM)])
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
                fee_bps: DEFAULT_FEE_BPS,
                pairs: vec![
                    PairInstantiateMsg {
                        native_denom: LUNC_DENOM.to_string(),
                        wrapped_token: wlunc.to_string(),
                        per_tx_wrap_limit: Uint128::from(1_000_000_000u128),
                        rolling_24h_wrap_limit: Uint128::from(1_000_000_000u128),
                    },
                    PairInstantiateMsg {
                        native_denom: USTC_DENOM.to_string(),
                        wrapped_token: wustc.to_string(),
                        per_tx_wrap_limit: Uint128::from(1_000_000_000u128),
                        rolling_24h_wrap_limit: Uint128::from(1_000_000_000u128),
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

    let cfg: cmm_native_wrap::msg::ConfigResponse = app
        .wrap()
        .query_wasm_smart(&wrap, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(cfg.pairs.len(), 2);
    assert_eq!(cfg.fee_bps, DEFAULT_FEE_BPS);

    let eff: cmm_native_wrap::msg::EffectiveWrapResponse = app
        .wrap()
        .query_wasm_smart(
            &wrap,
            &QueryMsg::EffectiveWrap {
                denom: LUNC_DENOM.to_string(),
            },
        )
        .unwrap();
    assert_eq!(eff.wrapped_token, wlunc.to_string());
    assert_eq!(eff.fee_chain_tax_bps, 50);
    assert_eq!(eff.fee_cmm_protocol_bps, 50);

    app.execute_contract(
        user.clone(),
        wrap,
        &ExecuteMsg::Wrap {},
        &[coin(1_000_000u128, LUNC_DENOM)],
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
    // 1% fee on 1_000_000 → 990_000 wLUNC minted
    assert_eq!(bal.balance, Uint128::from(990_000u128));
}
