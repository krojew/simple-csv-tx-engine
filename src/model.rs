use derive_more::Constructor;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Domain-specific client ID.
#[repr(transparent)]
#[derive(Deserialize, Serialize, Debug, Constructor, Eq, PartialEq)]
pub struct ClientId(u16);

/// Domain-specific transaction ID.
#[repr(transparent)]
#[derive(Deserialize, Serialize, Debug, Constructor, Eq, PartialEq)]
pub struct TransactionId(u32);

/// A single transaction to process.
#[derive(Deserialize, Debug, Eq, PartialEq)]
pub struct Transaction {
    pub r#type: String,

    #[serde(rename = "client")]
    pub client_id: ClientId,

    #[serde(rename = "tx")]
    pub transaction_id: TransactionId,

    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
}

/// Single client state after applying a list of transactions.
#[derive(Serialize, Debug)]
pub struct ClientState {
    #[serde(rename = "client")]
    pub client_id: ClientId,

    /// The total funds that are available for trading, staking, withdrawal, etc.
    #[serde(with = "rust_decimal::serde::str")]
    pub available: Decimal,

    /// The total funds that are held for dispute.
    #[serde(with = "rust_decimal::serde::str")]
    pub held: Decimal,

    /// The total funds that are available or held.
    #[serde(with = "rust_decimal::serde::str")]
    pub total: Decimal,

    /// Whether the account is locked.
    pub locked: bool,
}
