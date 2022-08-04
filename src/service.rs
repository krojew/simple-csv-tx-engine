use fxhash::FxHashMap;
use rust_decimal::Decimal;
use std::io::{stderr, BufWriter, Write};
use thiserror::Error;

use crate::exporter::ClientStateExporter;
use crate::importer::TransactionImporter;
use crate::model::{
    ClientId, ClientState, Transaction, TransactionError, TransactionId, TransactionType,
};

/// Possible processing errors.
#[derive(Error, Debug)]
pub enum ProcessingError {
    #[error("Transaction import error: {0}")]
    ImportError(#[source] anyhow::Error),
    #[error("Transaction export error: {0}")]
    ExportError(#[source] anyhow::Error),
    #[error("Missing amount for transaction: {0}")]
    MissingAmount(TransactionId),
    #[error("Transaction cannot be disputed again: {0}")]
    CannotDispute(TransactionId),
    #[error("Transaction cannot be resolved or charged back: {0}")]
    CannotResolveOrChargeBack(TransactionId),
    #[error("Error for transaction {transaction_id}: {error}")]
    TransactionError {
        transaction_id: TransactionId,
        #[source]
        error: TransactionError,
    },
}

/// Transaction processing service. Gathers transactions from a data source, computes resulting
/// client state, and writes data to given exporter. Intended to be used as a single-shot service
/// processing batches of transactions. Fallible data sources and sinks are allowed via the use of
/// an opaque error type.
pub struct TransactionProcessor<I: TransactionImporter, E: ClientStateExporter> {
    importer: I,
    exporter: E,
    context: ProcessingContext,
}

impl<I: TransactionImporter, E: ClientStateExporter> TransactionProcessor<I, E> {
    /// Creates a new processor with given importer and exporter.
    pub fn new(importer: I, exporter: E) -> Self {
        Self {
            importer,
            exporter,
            context: Default::default(),
        }
    }

    /// Processes a list of transactions and computes final client states.
    pub fn process_transactions(mut self) -> Result<(), ProcessingError> {
        self.import_and_process_transactions()?;
        self.export_client_states()
    }

    fn export_client_states(&mut self) -> Result<(), ProcessingError> {
        for client in self.context.clients.values() {
            self.exporter
                .serialize(&client.state)
                .map_err(ProcessingError::ExportError)?;
        }

        Ok(())
    }

    fn import_and_process_transactions(&mut self) -> Result<(), ProcessingError> {
        for transaction in self.importer.deserialize() {
            let transaction = transaction.map_err(ProcessingError::ImportError)?;

            // get current client state or create a new one
            let client = self
                .context
                .clients
                .entry(transaction.client_id)
                .or_insert_with(|| ClientInfo::new(ClientState::new(transaction.client_id)));

            let result = Self::process_transaction(client, &transaction);
            if let Err(error) = result {
                // a single invalid transaction should not cause all processing to stop
                // the requirements are unclear how to report the error, so simply aggregate the
                // errors and print a report to stderr
                self.context.transaction_errors.push(error);
            }
        }

        self.report_transaction_errors();

        Ok(())
    }

    fn report_transaction_errors(&self) {
        // print any tx errors encountered; use a lock to avoid locking on every write
        let stderr_lock = stderr().lock();
        let mut writer = BufWriter::new(stderr_lock);

        for error in &self.context.transaction_errors {
            // handling errors during error reporting is quite tricky, so for the sake of simplicity
            // in this example, we simply ignore it
            let _ = writeln!(&mut writer, "{}", error);
        }
    }

    fn process_transaction(
        client: &mut ClientInfo,
        transaction: &Transaction,
    ) -> Result<(), ProcessingError> {
        match transaction.r#type {
            TransactionType::Deposit => {
                let amount = extract_amount(transaction)?;

                map_from_transaction_error(transaction.transaction_id, || {
                    client.state.deposit(amount)
                })?;

                client.transactions.insert(
                    transaction.transaction_id,
                    TransactionInfo::new(amount, transaction.r#type),
                );
            }
            TransactionType::Withdrawal => {
                let amount = extract_amount(transaction)?;

                map_from_transaction_error(transaction.transaction_id, || {
                    client.state.withdraw(amount)
                })?;

                client.transactions.insert(
                    transaction.transaction_id,
                    TransactionInfo::new(amount, transaction.r#type),
                );
            }
            TransactionType::Dispute => {
                // we can ignore invalid transactions
                if let Some(original_transaction) =
                    client.transactions.get_mut(&transaction.transaction_id)
                {
                    if !original_transaction.can_dispute() {
                        return Err(ProcessingError::CannotDispute(transaction.transaction_id));
                    }

                    map_from_transaction_error(transaction.transaction_id, || {
                        client.state.dispute_deposit(original_transaction.amount)
                    })?;

                    original_transaction.state = TransactionState::Disputed;
                }
            }
            TransactionType::Resolve => {
                // we can ignore invalid transactions
                if let Some(original_transaction) =
                    client.transactions.get_mut(&transaction.transaction_id)
                {
                    if !original_transaction.can_resolve_or_charge_back() {
                        return Err(ProcessingError::CannotResolveOrChargeBack(
                            transaction.transaction_id,
                        ));
                    }

                    map_from_transaction_error(transaction.transaction_id, || {
                        client.state.resolve(original_transaction.amount)
                    })?;

                    // switch back to applied - can be disputed again
                    original_transaction.state = TransactionState::Applied;
                }
            }
            TransactionType::Chargeback => {
                // we can ignore invalid transactions
                if let Some(original_transaction) =
                    client.transactions.get_mut(&transaction.transaction_id)
                {
                    if !original_transaction.can_resolve_or_charge_back() {
                        return Err(ProcessingError::CannotResolveOrChargeBack(
                            transaction.transaction_id,
                        ));
                    }

                    map_from_transaction_error(transaction.transaction_id, || {
                        client.state.chargeback(original_transaction.amount)
                    })?;

                    original_transaction.state = TransactionState::ChargedBack;
                }
            }
        };

        Ok(())
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
enum TransactionState {
    Applied,
    Disputed,
    ChargedBack,
}

impl TransactionState {
    #[inline]
    fn can_dispute(self) -> bool {
        // we can only dispute applied transactions, not ones already disputed/charged back
        self == TransactionState::Applied
    }

    #[inline]
    fn can_resolve_or_charge_back(self) -> bool {
        self == TransactionState::Disputed
    }
}

#[derive(Copy, Clone)]
struct TransactionInfo {
    amount: Decimal,
    state: TransactionState,
    r#type: TransactionType,
}

impl TransactionInfo {
    #[inline]
    fn new(amount: Decimal, r#type: TransactionType) -> Self {
        Self {
            amount,
            state: TransactionState::Applied,
            r#type,
        }
    }

    #[inline]
    fn can_dispute(&self) -> bool {
        // the requirements suggest we handle only deposit disputes - see the README for details
        self.r#type == TransactionType::Deposit && self.state.can_dispute()
    }

    #[inline]
    fn can_resolve_or_charge_back(&self) -> bool {
        self.state.can_resolve_or_charge_back()
    }
}

// client state with all referenced transactions
struct ClientInfo {
    state: ClientState,
    transactions: FxHashMap<TransactionId, TransactionInfo>,
}

impl ClientInfo {
    #[inline]
    fn new(state: ClientState) -> Self {
        Self {
            state,
            transactions: Default::default(),
        }
    }
}

#[derive(Default)]
struct ProcessingContext {
    clients: FxHashMap<ClientId, ClientInfo>,
    transaction_errors: Vec<ProcessingError>,
}

#[inline]
fn extract_amount(transaction: &Transaction) -> Result<Decimal, ProcessingError> {
    transaction
        .amount
        .ok_or(ProcessingError::MissingAmount(transaction.transaction_id))
}

#[inline]
fn map_from_transaction_error<F: FnOnce() -> Result<(), TransactionError>>(
    transaction_id: TransactionId,
    action: F,
) -> Result<(), ProcessingError> {
    // simple helper for mapping transaction errors to processing errors
    action().map_err(|error| ProcessingError::TransactionError {
        transaction_id,
        error,
    })
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;
    use std::io::Read;

    use crate::exporter::ClientStateExporter;
    use crate::importer::TransactionCsvImporter;
    use crate::model::{ClientId, ClientState};
    use crate::service::TransactionProcessor;

    #[derive(Clone, Default)]
    struct CachingExporter {
        client_states: Vec<ClientState>,
    }

    impl ClientStateExporter for &mut CachingExporter {
        fn serialize(&mut self, client_state: &ClientState) -> anyhow::Result<()> {
            self.client_states.push(*client_state);
            Ok(())
        }
    }

    fn create_importer_and_exporter<R: Read>(
        csv: R,
    ) -> (TransactionCsvImporter<R>, CachingExporter) {
        (
            TransactionCsvImporter::from_reader(csv),
            CachingExporter::default(),
        )
    }

    #[test]
    fn should_apply_transactions_for_single_client() {
        let csv = "type,client,tx,amount
deposit,1,1,2
withdrawal,1,2,1
dispute,1,1,
resolve,1,1,
dispute,1,1,
chargeback,1,1,
";

        let (importer, mut exporter) = create_importer_and_exporter(csv.as_bytes());
        let processor = TransactionProcessor::new(importer, &mut exporter);
        processor.process_transactions().unwrap();

        assert_eq!(exporter.client_states.len(), 1);
        assert_eq!(exporter.client_states[0].client_id(), ClientId::new(1));
        assert_eq!(exporter.client_states[0].total(), Decimal::from(-1));
        assert!(exporter.client_states[0].held().is_zero());
        assert!(exporter.client_states[0].locked());
    }

    #[test]
    fn should_apply_transactions_for_multiple_clients() {
        let csv = "type,client,tx,amount
deposit,1,1,2
deposit,2,1,3
withdrawal,1,2,1
dispute,2,1,
";

        let (importer, mut exporter) = create_importer_and_exporter(csv.as_bytes());
        let processor = TransactionProcessor::new(importer, &mut exporter);
        processor.process_transactions().unwrap();

        assert_eq!(exporter.client_states.len(), 2);

        let client_1 = exporter
            .client_states
            .iter()
            .find(|client| client.client_id() == ClientId::new(1))
            .unwrap();

        let client_2 = exporter
            .client_states
            .iter()
            .find(|client| client.client_id() == ClientId::new(2))
            .unwrap();

        assert_eq!(client_1.total(), Decimal::from(1));
        assert!(client_1.held().is_zero());
        assert!(!client_1.locked());

        assert_eq!(client_2.total(), Decimal::from(3));
        assert_eq!(client_2.held(), Decimal::from(3));
        assert!(!client_2.locked());
    }

    #[test]
    fn should_ignore_invalid_disputes() {
        let csv = "type,client,tx,amount
deposit,1,1,2
dispute,1,2,
";

        let (importer, mut exporter) = create_importer_and_exporter(csv.as_bytes());
        let processor = TransactionProcessor::new(importer, &mut exporter);
        processor.process_transactions().unwrap();

        assert_eq!(exporter.client_states.len(), 1);
        assert_eq!(exporter.client_states[0].client_id(), ClientId::new(1));
        assert_eq!(exporter.client_states[0].total(), Decimal::from(2));
        assert!(exporter.client_states[0].held().is_zero());
        assert!(!exporter.client_states[0].locked());
    }

    #[test]
    fn should_not_dispute_withdrawal() {
        let csv = "type,client,tx,amount
deposit,1,1,2
withdrawal,1,2,2
dispute,1,2,
";

        let (importer, mut exporter) = create_importer_and_exporter(csv.as_bytes());
        let processor = TransactionProcessor::new(importer, &mut exporter);
        processor.process_transactions().unwrap();

        assert_eq!(exporter.client_states.len(), 1);
        assert_eq!(exporter.client_states[0].client_id(), ClientId::new(1));
        assert!(exporter.client_states[0].total().is_zero());
        assert!(exporter.client_states[0].held().is_zero());
        assert!(!exporter.client_states[0].locked());
    }
}
