use anyhow::{anyhow, Context, Result};
use csv::Writer;
use std::env;
use std::io::stdout;

use simple_csv_tx_engine::importer::TransactionCsvImporter;
use simple_csv_tx_engine::service::TransactionProcessor;

fn main() -> Result<()> {
    // for more complex/generic apps, we should use a crate like `clap` for argument handling, but
    // in this case, our app interface is well-defined and consistent + we're prioritizing speed
    let input_file = env::args()
        .nth(1)
        .ok_or_else(|| anyhow!("Missing input file!"))?;

    // import from our input file; export to stdout by default
    let importer = TransactionCsvImporter::from_path(&input_file)?;

    // note: we're locking stdout upfront to avoid locking on every write; there's no need to add
    // buffering, since `csv` already does that
    let exporter = Writer::from_writer(stdout().lock());

    let processor = TransactionProcessor::new(importer, exporter);
    processor
        .process_transactions()
        .with_context(|| format!("Error processing {}!", input_file))
}
