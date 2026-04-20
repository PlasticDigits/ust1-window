//! **INV-SWAP-001 / INV-LIMIT-001**: End-to-end deposit and withdraw on mock chain.
use cosmwasm_std::{to_json_binary, Addr, Empty, Uint128};
use cw20::{Cw20ExecuteMsg, MinterResponse};
use cw_multi_test::{App, ContractWrapper, Executor};
use ust1_common::RATE_SCALE;
use ust1_oracle::msg as oracle_msg;
use ust1_window::msg as window_msg;

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

#[test]
fn deposit_and_withdraw_round_trip() {
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
            &window_msg::InstantiateMsg {
                governance: owner.to_string(),
                oracle: oracle.to_string(),
                vfdusd_token: vfdusd.to_string(),
                ust1_token: ust1.to_string(),
                fee_bps: 50,
                per_tx_ust1_limit: Uint128::from(500_000_000u128),
                rolling_24h_ust1_limit: Uint128::from(2_500_000_000u128),
            },
            &[],
            "window",
            None,
        )
        .unwrap();

    // Allow window to mint UST1
    app.execute_contract(
        owner.clone(),
        ust1.clone(),
        &cw20_mintable::msg::ExecuteMsg::AddMinter {
            minter: window.to_string(),
        },
        &[],
    )
    .unwrap();

    // Seed user with vFDUSD
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

    let hook = window_msg::Cw20HookMsg::Deposit {};
    app.execute_contract(
        user.clone(),
        vfdusd.clone(),
        &Cw20ExecuteMsg::Send {
            contract: window.to_string(),
            amount: Uint128::from(1_000_000u128),
            msg: to_json_binary(&hook).unwrap(),
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
    assert!(bal.balance > Uint128::zero());

    // Withdraw: send UST1 back for vFDUSD
    let w_hook = window_msg::Cw20HookMsg::Withdraw {
        min_vfdusd_out: Uint128::zero(),
    };
    app.execute_contract(
        user.clone(),
        ust1.clone(),
        &Cw20ExecuteMsg::Send {
            contract: window.to_string(),
            amount: bal.balance,
            msg: to_json_binary(&w_hook).unwrap(),
        },
        &[],
    )
    .unwrap();

    let vbal: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            vfdusd.clone(),
            &cw20::Cw20QueryMsg::Balance {
                address: user.to_string(),
            },
        )
        .unwrap();
    assert!(vbal.balance > Uint128::zero());
}
