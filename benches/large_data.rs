use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::prelude::*;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use simple_csv_tx_engine::exporter::ClientStateExporter;
use simple_csv_tx_engine::importer::TransactionImporter;
use simple_csv_tx_engine::model::{
    ClientId, ClientState, Transaction, TransactionId, TransactionType,
};
use simple_csv_tx_engine::service::TransactionProcessor;

struct PredefinedTransactionImporter {
    transactions: Vec<Transaction>,
}

impl TransactionImporter for &PredefinedTransactionImporter {
    fn deserialize(&mut self) -> Box<dyn Iterator<Item = anyhow::Result<Transaction>> + '_> {
        Box::new(self.transactions.iter().map(|tx| Ok(*tx)))
    }
}

struct NullClientStateExporter;

impl ClientStateExporter for NullClientStateExporter {
    fn serialize(&mut self, _client_state: &ClientState) -> anyhow::Result<()> {
        Ok(())
    }
}

fn create_transaction_type(rng: &mut impl Rng) -> TransactionType {
    if rng.gen_bool(0.5) {
        TransactionType::Deposit
    } else {
        TransactionType::Withdrawal
    }
}

fn create_transaction_id(index: u64, r#type: TransactionType) -> TransactionId {
    match r#type {
        TransactionType::Deposit | TransactionType::Withdrawal => TransactionId::new(index as u32),
        _ => unreachable!(),
    }
}

fn create_sample_transactions(size: u64) -> PredefinedTransactionImporter {
    let mut transactions = Vec::with_capacity(size as usize);

    // use hardcoded values for deterministic benches
    let mut rng = SmallRng::seed_from_u64(0xbeef00666);
    for i in 0..size {
        let r#type = create_transaction_type(&mut rng);
        let amount_delta = if r#type == TransactionType::Deposit {
            10000.
        } else {
            1.
        };

        transactions.push(Transaction {
            r#type,
            client_id: ClientId::new(rng.gen_range(0..50)),
            transaction_id: create_transaction_id(i, r#type),
            amount: Decimal::from_f32(rng.gen_range(0f32..((i + 1) * 10) as f32) + amount_delta),
        });
    }

    PredefinedTransactionImporter { transactions }
}

fn large_data(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_data");

    for size in [100, 1000, 10000, 1000000].iter().copied() {
        let importer = create_sample_transactions(size);

        group.throughput(Throughput::Elements(size));
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &importer,
            |b, importer| {
                b.iter(|| {
                    let processor = TransactionProcessor::new(importer, NullClientStateExporter);
                    processor
                        .process_transactions()
                        .expect("Unexpected processing error!");
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, large_data);
criterion_main!(benches);
