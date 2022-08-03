use csv::{Error, Reader, ReaderBuilder, Trim};
use std::fmt::Display;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::model::Transaction;

/// Abstract transaction importer.
pub trait TransactionImporter {
    /// Returns an iterator over deserialized transactions.
    fn deserialize(&mut self) -> Box<dyn Iterator<Item = anyhow::Result<Transaction>> + '_>;
}

/// Transaction importer from a CSV reader. Takes care of header/data normalization (whitespace
/// support).
pub struct TransactionCsvImporter<R: Read> {
    csv_reader: Reader<R>,
}

impl<R: Read> TransactionImporter for TransactionCsvImporter<R> {
    fn deserialize(&mut self) -> Box<dyn Iterator<Item = anyhow::Result<Transaction>> + '_> {
        Box::new(
            self.csv_reader
                .deserialize()
                .map(|tx| tx.map_err(|error| error.into())),
        )
    }
}

impl TransactionCsvImporter<File> {
    /// Creates a new importer from given input file.
    pub fn from_path<P: AsRef<Path> + Display>(input_file: P) -> Result<Self, Error> {
        Self::configure_reader_builder(&mut ReaderBuilder::new())
            .from_path(&input_file)
            .map(|csv_reader| Self { csv_reader })
    }
}

impl<R: Read> TransactionCsvImporter<R> {
    /// Creates a new importer from given input `Reader`.
    #[cfg(test)]
    pub(crate) fn from_reader(reader: R) -> Self {
        let csv_reader =
            Self::configure_reader_builder(&mut ReaderBuilder::new()).from_reader(reader);

        Self { csv_reader }
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

    use crate::importer::{TransactionCsvImporter, TransactionImporter};
    use crate::model::{ClientId, Transaction, TransactionId, TransactionType};

    fn create_test_transactions() -> Vec<Transaction> {
        vec![
            Transaction {
                r#type: TransactionType::Deposit,
                client_id: ClientId::new(1),
                transaction_id: TransactionId::new(1),
                amount: Some(Decimal::from_f32(1.).unwrap()),
            },
            Transaction {
                r#type: TransactionType::Withdrawal,
                client_id: ClientId::new(1),
                transaction_id: TransactionId::new(4),
                amount: Some(Decimal::from_f32(1.5).unwrap()),
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
