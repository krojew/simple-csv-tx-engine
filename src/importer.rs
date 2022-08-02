use csv::{Error, Reader, ReaderBuilder, Trim};
use std::fmt::Display;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::model::Transaction;

/// Possible importer errors.
#[derive(Debug, thiserror::Error)]
pub enum ImporterError {
    /// An error originating from parsing CSV data.
    #[error("CSV error: {0}")]
    CsvError(#[from] Error),
}

/// Transaction importer from a CSV reader. Takes care of header/data normalization (whitespace
/// support).
pub struct TransactionCsvImporter<R: Read> {
    csv_reader: Reader<R>,
}

impl TransactionCsvImporter<File> {
    /// Creates a new importer from given input file.
    pub fn from_path<P: AsRef<Path> + Display>(input_file: P) -> Result<Self, ImporterError> {
        Self::configure_reader_builder(&mut ReaderBuilder::new())
            .from_path(&input_file)
            .map(|csv_reader| Self { csv_reader })
            .map_err(|error| error.into())
    }
}

impl<R: Read> TransactionCsvImporter<R> {
    /// Creates a new importer from given input `Reader`.
    #[cfg(test)]
    fn from_reader(reader: R) -> Self {
        let csv_reader =
            Self::configure_reader_builder(&mut ReaderBuilder::new()).from_reader(reader);

        Self { csv_reader }
    }

    /// Returns an iterator over deserialized transactions.
    #[inline]
    pub fn deserialize(&mut self) -> impl Iterator<Item = Result<Transaction, ImporterError>> + '_ {
        self.csv_reader
            .deserialize()
            .map(|tx| tx.map_err(|error| error.into()))
    }

    fn configure_reader_builder(builder: &mut ReaderBuilder) -> &mut ReaderBuilder {
        // headers and data can contain whitespace sometimes, so we need to trim them
        builder.trim(Trim::All)
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;
    use rust_decimal::prelude::*;

    use crate::importer::TransactionCsvImporter;
    use crate::model::{ClientId, Transaction, TransactionId, TransactionType};

    fn create_test_transactions() -> Vec<Transaction> {
        vec![
            Transaction {
                r#type: TransactionType::Deposit,
                client_id: ClientId::new(1),
                transaction_id: TransactionId::new(1),
                amount: Decimal::from_f32(1.).unwrap(),
            },
            Transaction {
                r#type: TransactionType::Withdrawal,
                client_id: ClientId::new(1),
                transaction_id: TransactionId::new(4),
                amount: Decimal::from_f32(1.5).unwrap(),
            },
        ]
    }

    #[test]
    fn should_parse_csv_without_whitespace() {
        let csv = "type,client,tx,amount
deposit,1,1,1.0
withdrawal,1,4,1.5
";

        let mut importer = TransactionCsvImporter::from_reader(csv.as_bytes());
        let transactions: Vec<_> = importer.deserialize().try_collect().unwrap();
        assert_eq!(transactions, create_test_transactions());
    }

    #[test]
    fn should_parse_csv_with_whitespace() {
        let csv = " type, client, tx ,amount
deposit, 1, 1, 1.0
withdrawal, 1, 4 , 1.5
";

        let mut importer = TransactionCsvImporter::from_reader(csv.as_bytes());
        let transactions: Vec<_> = importer.deserialize().try_collect().unwrap();
        assert_eq!(transactions, create_test_transactions());
    }
}
