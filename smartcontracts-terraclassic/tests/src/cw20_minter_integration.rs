//! **INV-MINTER-001** (M-8): pinned `cw20-mintable` `UpdateMinter` clears stale primary
//! entries from the additional `MINTERS` map.
//!
//! Cross-links: [ust1-window#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25),
//! [ust1-window#28](https://gitlab.com/PlasticDigits/ust1-window/-/issues/28),
//! [`skills/audit-hardening-bundle`](../../../skills/audit-hardening-bundle/SKILL.md).

use cosmwasm_std::{Addr, Empty, Uint128};
use cw20::MinterResponse;
use cw20_mintable::msg::{ExecuteMsg, MintersResponse, QueryMsg};
use cw_multi_test::{App, ContractWrapper, Executor};

fn cw20_mintable_contract() -> Box<dyn cw_multi_test::Contract<Empty>> {
    let c = ContractWrapper::new(
        cw20_mintable::contract::execute,
        cw20_mintable::contract::instantiate,
        cw20_mintable::contract::query,
    );
    Box::new(c)
}

fn query_minters(app: &App, token: &Addr) -> MintersResponse {
    app.wrap()
        .query_wasm_smart(
            token,
            &QueryMsg::Minters {
                start_after: None,
                limit: None,
            },
        )
        .unwrap()
}

fn assert_unauthorized(err: cw_multi_test::error::AnyError) {
    assert!(
        err.root_cause().to_string().contains("Unauthorized"),
        "unexpected err: {err}"
    );
}

/// **INV-MINTER-001**: `UpdateMinter { new_minter: None }` drops a dual-listed primary from
/// `MINTERS` while unrelated additional minters remain and can still mint.
#[test]
fn inv_minter_001_update_minter_none_clears_minters_map() {
    let mut app = App::default();
    let owner = Addr::unchecked("owner");
    let window = Addr::unchecked("window");
    let extra = Addr::unchecked("extra");
    let recipient = Addr::unchecked("recipient");

    let cw20_id = app.store_code(cw20_mintable_contract());
    let token = app
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

    for minter in [&window, &owner, &extra] {
        app.execute_contract(
            owner.clone(),
            token.clone(),
            &ExecuteMsg::AddMinter {
                minter: minter.to_string(),
            },
            &[],
        )
        .unwrap();
    }

    let minters = query_minters(&app, &token);
    assert!(minters.minters.contains(&owner.to_string()));
    assert!(minters.minters.contains(&extra.to_string()));
    assert!(minters.minters.contains(&window.to_string()));

    app.execute_contract(
        owner.clone(),
        token.clone(),
        &ExecuteMsg::UpdateMinter { new_minter: None },
        &[],
    )
    .unwrap();

    let minters = query_minters(&app, &token);
    assert!(
        !minters.minters.contains(&owner.to_string()),
        "primary must be removed from MINTERS on UpdateMinter(None): {:?}",
        minters.minters
    );
    assert!(minters.minters.contains(&extra.to_string()));
    assert!(minters.minters.contains(&window.to_string()));

    let err = app
        .execute_contract(
            owner.clone(),
            token.clone(),
            &ExecuteMsg::Mint {
                recipient: recipient.to_string(),
                amount: Uint128::new(1),
            },
            &[],
        )
        .unwrap_err();
    assert_unauthorized(err);

    app.execute_contract(
        extra.clone(),
        token.clone(),
        &ExecuteMsg::Mint {
            recipient: recipient.to_string(),
            amount: Uint128::new(1),
        },
        &[],
    )
    .unwrap();
}

/// **INV-MINTER-001**: rotating primary via `UpdateMinter(Some(new))` also clears the old
/// primary from `MINTERS`; the new primary can mint and re-add the old address.
#[test]
fn inv_minter_001_update_minter_some_clears_old_primary_from_minters_map() {
    let mut app = App::default();
    let old_primary = Addr::unchecked("owner");
    let new_primary = Addr::unchecked("new_primary");
    let recipient = Addr::unchecked("recipient");

    let cw20_id = app.store_code(cw20_mintable_contract());
    let token = app
        .instantiate_contract(
            cw20_id,
            old_primary.clone(),
            &cw20_mintable::msg::InstantiateMsg {
                name: "UST1".into(),
                symbol: "UST1".into(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: old_primary.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            &[],
            "ust1",
            None,
        )
        .unwrap();

    app.execute_contract(
        old_primary.clone(),
        token.clone(),
        &ExecuteMsg::AddMinter {
            minter: old_primary.to_string(),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        old_primary.clone(),
        token.clone(),
        &ExecuteMsg::UpdateMinter {
            new_minter: Some(new_primary.to_string()),
        },
        &[],
    )
    .unwrap();

    let minters = query_minters(&app, &token);
    assert!(!minters.minters.contains(&old_primary.to_string()));

    let err = app
        .execute_contract(
            old_primary.clone(),
            token.clone(),
            &ExecuteMsg::Mint {
                recipient: recipient.to_string(),
                amount: Uint128::new(1),
            },
            &[],
        )
        .unwrap_err();
    assert_unauthorized(err);

    app.execute_contract(
        new_primary.clone(),
        token.clone(),
        &ExecuteMsg::Mint {
            recipient: recipient.to_string(),
            amount: Uint128::new(1),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        new_primary.clone(),
        token.clone(),
        &ExecuteMsg::AddMinter {
            minter: old_primary.to_string(),
        },
        &[],
    )
    .unwrap();

    let minters = query_minters(&app, &token);
    assert!(minters.minters.contains(&old_primary.to_string()));
}
