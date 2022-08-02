use anyhow::{anyhow, Context, Result};
use std::env;

use simple_csv_tx_engine::engine::process_transactions;
use simple_csv_tx_engine::importer::TransactionCsvImporter;

fn main() -> Result<()> {
    // For more complex/generic apps, we should use a crate like `clap` for argument handling, but
    // in this case, our app interface is well-defined and consistent + we're prioritizing speed.
    let input_file = env::args()
        .nth(1)
        .ok_or_else(|| anyhow!("Missing input file!"))?;

    let mut importer = TransactionCsvImporter::from_path(&input_file)?;

    let transactions = importer
        .deserialize()
        .map(|tx| tx.map_err(|error| error.into()));

    let client_states = process_transactions(transactions)
        .with_context(|| format!("Cannot process input file: {}", input_file))?;

    Ok(())
}
