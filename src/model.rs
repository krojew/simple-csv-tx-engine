use derive_more::{Constructor, Display};
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// Domain-specific client ID.
#[repr(transparent)]
#[derive(Deserialize, Serialize, Debug, Constructor, Eq, PartialEq, Display, Copy, Clone, Hash)]
pub struct ClientId(u16);

/// Domain-specific transaction ID.
#[repr(transparent)]
#[derive(Deserialize, Serialize, Debug, Constructor, Eq, PartialEq, Display, Copy, Clone, Hash)]
pub struct TransactionId(u32);

/// Possible transaction type.
#[derive(Deserialize, Debug, Eq, PartialEq, Copy, Clone)]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

/// A single transaction to process.
#[derive(Deserialize, Debug, Eq, PartialEq, Copy, Clone)]
pub struct Transaction {
    pub r#type: TransactionType,

    #[serde(rename = "client")]
    pub client_id: ClientId,

    #[serde(rename = "tx")]
    pub transaction_id: TransactionId,

    #[serde(deserialize_with = "deserialize_optional_decimal")]
    pub amount: Option<Decimal>,
}

/// Errors related to invalid transaction operations.
#[derive(Debug, Error, Copy, Clone, PartialEq, Eq)]
pub enum TransactionError {
    #[error("Invalid transaction amount: {0}")]
    InvalidAmount(Decimal),
    #[error("Insufficient funds to perform the operation!")]
    InsufficientFunds,
    #[error("Operation not permitted on a locked account!")]
    AccountLocked,
}

/// Single client state after applying a list of transactions.
#[derive(Serialize, Debug, Copy, Clone)]
pub struct ClientState {
    #[serde(rename = "client")]
    client_id: ClientId,

    /// The total funds that are available for trading, staking, withdrawal, etc.
    #[serde(serialize_with = "serialize_with_fixed_precision")]
    available: Decimal,

    /// The total funds that are held for dispute.
    #[serde(serialize_with = "serialize_with_fixed_precision")]
    held: Decimal,

    /// The total funds that are available or held.
    #[serde(serialize_with = "serialize_with_fixed_precision")]
    total: Decimal,

    /// Whether the account is locked.
    locked: bool,
}

impl ClientState {
    /// Creates a new state with no funds.
    #[inline]
    pub fn new(client_id: ClientId) -> Self {
        Self {
            client_id,
            available: Default::default(),
            held: Default::default(),
            total: Default::default(),
            locked: false,
        }
    }

    /// Deposits some funds into the account, increasing the available amount.
    pub fn deposit(&mut self, amount: Decimal) -> Result<(), TransactionError> {
        if amount.is_sign_negative() {
            return Err(TransactionError::InvalidAmount(amount));
        }

        self.available += amount;
        self.total += amount;

        Ok(())
    }

    /// Withdraws funds from the amount. Does not allow for negative balance.
    pub fn withdraw(&mut self, amount: Decimal) -> Result<(), TransactionError> {
        if self.locked {
            return Err(TransactionError::AccountLocked);
        }

        if amount.is_sign_negative() {
            return Err(TransactionError::InvalidAmount(amount));
        }

        if self.available < amount {
            return Err(TransactionError::InsufficientFunds);
        }

        self.available -= amount;
        self.total -= amount;

        Ok(())
    }

    /// Disputes a transaction with the given amount. Currently, only disputing deposits is
    /// supported due to incoming transaction data description:
    /// *clients available funds should decrease by the amount disputed, their held funds should
    /// increase by the amount disputed, while their total funds should remain the same*.
    pub fn dispute(&mut self, amount: Decimal) -> Result<(), TransactionError> {
        // since we can dispute deposits or withdrawals,
        if amount.is_sign_negative() {
            return Err(TransactionError::InvalidAmount(amount));
        }

        self.available -= amount;
        self.held += amount;

        Ok(())
    }

    /// Resolves a transaction with the given amount.
    pub fn resolve(&mut self, amount: Decimal) -> Result<(), TransactionError> {
        if amount.is_sign_negative() {
            return Err(TransactionError::InvalidAmount(amount));
        }

        if self.held < amount {
            return Err(TransactionError::InsufficientFunds);
        }

        self.available += amount;
        self.held -= amount;

        Ok(())
    }

    /// Issues a chargeback on a disputed transaction with a given amount. Lock the account, so no
    /// further deposits/withdrawals can take place.
    pub fn chargeback(&mut self, amount: Decimal) -> Result<(), TransactionError> {
        if amount.is_sign_negative() {
            return Err(TransactionError::InvalidAmount(amount));
        }

        if self.held < amount {
            return Err(TransactionError::InsufficientFunds);
        }

        self.held -= amount;
        self.total -= amount;
        self.locked = true;

        Ok(())
    }

    #[inline]
    pub fn client_id(&self) -> ClientId {
        self.client_id
    }

    #[cfg(test)]
    pub(crate) fn held(&self) -> Decimal {
        self.held
    }

    #[cfg(test)]
    pub(crate) fn total(&self) -> Decimal {
        self.total
    }

    #[cfg(test)]
    pub(crate) fn locked(&self) -> bool {
        self.locked
    }
}

fn serialize_with_fixed_precision<S: Serializer>(
    value: &Decimal,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format!("{:.4}", value))
}

fn deserialize_optional_decimal<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Decimal>, D::Error> {
    // serde doesn't yet support custom (de)serialization wrapped in an Option, so need to work
    // around that
    #[derive(Deserialize)]
    #[repr(transparent)]
    struct DecimalWrapper(#[serde(with = "rust_decimal::serde::str")] Decimal);
    Option::deserialize(deserializer).map(|value| value.map(|DecimalWrapper(value)| value))
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use crate::model::{ClientId, ClientState, TransactionError};

    #[test]
    fn should_deposit_funds() {
        let mut state = ClientState::new(ClientId::new(2));
        state.deposit(Decimal::from(3)).unwrap();

        assert_eq!(state.available, Decimal::from(3));
        assert!(state.held.is_zero());
        assert_eq!(state.total, Decimal::from(3));
    }

    #[test]
    fn should_not_deposit_negative_funds() {
        let mut state = ClientState::new(ClientId::new(2));
        assert_eq!(
            state.deposit(Decimal::from(-3)).unwrap_err(),
            TransactionError::InvalidAmount(Decimal::from(-3))
        );

        assert!(state.available.is_zero());
        assert!(state.held.is_zero());
        assert!(state.total.is_zero());
    }

    #[test]
    fn should_withdraw_funds() {
        let mut state = ClientState::new(ClientId::new(2));
        state.deposit(Decimal::from(4)).unwrap();
        state.withdraw(Decimal::from(3)).unwrap();

        assert_eq!(state.available, Decimal::from(1));
        assert!(state.held.is_zero());
        assert_eq!(state.total, Decimal::from(1));
    }

    #[test]
    fn should_not_withdraw_negative_funds() {
        let mut state = ClientState::new(ClientId::new(2));
        state.deposit(Decimal::from(4)).unwrap();
        assert_eq!(
            state.withdraw(Decimal::from(-3)).unwrap_err(),
            TransactionError::InvalidAmount(Decimal::from(-3))
        );

        assert_eq!(state.available, Decimal::from(4));
        assert!(state.held.is_zero());
        assert_eq!(state.total, Decimal::from(4));
    }

    #[test]
    fn should_not_withdraw_missing_funds() {
        let mut state = ClientState::new(ClientId::new(2));
        assert_eq!(
            state.withdraw(Decimal::from(3)).unwrap_err(),
            TransactionError::InsufficientFunds
        );

        assert!(state.available.is_zero());
        assert!(state.held.is_zero());
        assert!(state.total.is_zero());
    }

    #[test]
    fn should_not_withdraw_from_locked_account() {
        let mut state = ClientState::new(ClientId::new(2));
        state.deposit(Decimal::from(4)).unwrap();
        state.locked = true;

        assert_eq!(
            state.withdraw(Decimal::from(3)).unwrap_err(),
            TransactionError::AccountLocked
        );

        assert_eq!(state.available, Decimal::from(4));
        assert!(state.held.is_zero());
        assert_eq!(state.total, Decimal::from(4));
    }

    #[test]
    fn should_dispute_funds() {
        let mut state = ClientState::new(ClientId::new(2));
        state.deposit(Decimal::from(4)).unwrap();
        state.dispute(Decimal::from(3)).unwrap();

        assert_eq!(state.available, Decimal::from(1));
        assert_eq!(state.held, Decimal::from(3));
        assert_eq!(state.total, Decimal::from(4));
    }

    #[test]
    fn should_not_dispute_negative_funds() {
        let mut state = ClientState::new(ClientId::new(2));
        state.deposit(Decimal::from(4)).unwrap();
        assert_eq!(
            state.dispute(Decimal::from(-3)).unwrap_err(),
            TransactionError::InvalidAmount(Decimal::from(-3))
        );

        assert_eq!(state.available, Decimal::from(4));
        assert!(state.held.is_zero());
        assert_eq!(state.available, Decimal::from(4));
    }

    #[test]
    fn should_resolve_funds() {
        let mut state = ClientState::new(ClientId::new(2));
        state.deposit(Decimal::from(4)).unwrap();
        state.dispute(Decimal::from(3)).unwrap();
        state.resolve(Decimal::from(3)).unwrap();

        assert_eq!(state.available, Decimal::from(4));
        assert!(state.held.is_zero());
        assert_eq!(state.total, Decimal::from(4));
    }

    #[test]
    fn should_not_resolve_negative_funds() {
        let mut state = ClientState::new(ClientId::new(2));
        state.deposit(Decimal::from(4)).unwrap();
        state.dispute(Decimal::from(3)).unwrap();
        assert_eq!(
            state.resolve(Decimal::from(-3)).unwrap_err(),
            TransactionError::InvalidAmount(Decimal::from(-3))
        );

        assert_eq!(state.available, Decimal::from(1));
        assert_eq!(state.held, Decimal::from(3));
        assert_eq!(state.total, Decimal::from(4));
    }

    #[test]
    fn should_not_resolve_missing_funds() {
        let mut state = ClientState::new(ClientId::new(2));
        state.deposit(Decimal::from(4)).unwrap();
        state.dispute(Decimal::from(3)).unwrap();
        assert_eq!(
            state.resolve(Decimal::from(4)).unwrap_err(),
            TransactionError::InsufficientFunds
        );

        assert_eq!(state.available, Decimal::from(1));
        assert_eq!(state.held, Decimal::from(3));
        assert_eq!(state.total, Decimal::from(4));
    }

    #[test]
    fn should_charge_back_funds() {
        let mut state = ClientState::new(ClientId::new(2));
        state.deposit(Decimal::from(4)).unwrap();
        state.dispute(Decimal::from(3)).unwrap();
        state.chargeback(Decimal::from(3)).unwrap();

        assert_eq!(state.available, Decimal::from(1));
        assert!(state.held.is_zero());
        assert_eq!(state.total, Decimal::from(1));
        assert!(state.locked);
    }

    #[test]
    fn should_not_charge_back_negative_funds() {
        let mut state = ClientState::new(ClientId::new(2));
        state.deposit(Decimal::from(4)).unwrap();
        state.dispute(Decimal::from(3)).unwrap();
        assert_eq!(
            state.chargeback(Decimal::from(-3)).unwrap_err(),
            TransactionError::InvalidAmount(Decimal::from(-3))
        );

        assert_eq!(state.available, Decimal::from(1));
        assert_eq!(state.held, Decimal::from(3));
        assert_eq!(state.total, Decimal::from(4));
        assert!(!state.locked);
    }

    #[test]
    fn should_not_charge_back_missing_funds() {
        let mut state = ClientState::new(ClientId::new(2));
        state.deposit(Decimal::from(4)).unwrap();
        state.dispute(Decimal::from(3)).unwrap();
        assert_eq!(
            state.chargeback(Decimal::from(4)).unwrap_err(),
            TransactionError::InsufficientFunds
        );

        assert_eq!(state.available, Decimal::from(1));
        assert_eq!(state.held, Decimal::from(3));
        assert_eq!(state.total, Decimal::from(4));
        assert!(!state.locked);
    }
}
