use thiserror::Error;

use crate::model::Transaction;

/// Possible processing errors.
#[derive(Error, Debug)]
pub enum ProcessingError {}

/// Processes a list of transactions and computes final client states. Fallible data sources are
/// allowed via the use of an opaque error type.
pub fn process_transactions(
    transactions: impl Iterator<Item = anyhow::Result<Transaction>>,
) -> Result<(), ProcessingError> {
    Ok(())
}

#[cfg(test)]
mod tests {}
