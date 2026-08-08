//! Integration tests for ust1-oracle + ust1-window (cw-multi-test).

#[cfg(test)]
mod stub_treasury;

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod treasury_schema;

#[cfg(test)]
mod real_treasury_integration;

#[cfg(test)]
mod oracle_policy_tests;

#[cfg(test)]
mod cmm_native_wrap_integration;
