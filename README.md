Simple transaction engine taking sample data from a CSV file, and dumping resulting state to stdout as CSV.

Input data is being supplied via a `TransactionImporter` to a `TransactionProcessor`, which is then consumed
and aggregated into final client states, and returned to a `ClientStateExporter`. This simple ETL-like 
approach shows how to configure a generic *"fire-and-forget"* processing pipeline with little effort. Data
model correctness, as well as error handling is enforced by language rules and library APIs. A suite of tests
guards invariants from breaking (some assumptions have been made regarding undocumented cases).

While this is perfectly fine for processing single batches of data, stateless domain services connected to
the outside world via classic hexagonal ports and adapters (data input/output, storage, caching, etc.) would
be better suited for continuous data processing.

Processing large volumes of data should use stream processing for high scalability and efficiency (e.g.
**Kafka Streams**, although KS DSL is not available for Rust yet). Since the application is IO bound and
processing itself is quite trivial, using a multithreaded solution might not yield positive results, at
least for small datasets or low number of distinct clients. A more advanced version might switch between
sequential and parallel solution based on a runtime parameter or a cost heuristic.

Transactions can have the following outcomes for a given client:

- *Deposit*: increase the available funds. Doesn't require the account to be unlocked.
- *Withdrawal*: decrease the available funds, if the account is not locked.
- *Dispute*: holds funds associated with a given transaction, until the dispute is resolved in any way.
  Doesn't require the account to be unlocked. Note: only deposit transaction disputes are currently
  supported due to incoming data description: *clients available funds should decrease by the amount
  disputed, their held funds should increase by the amount disputed, while their total funds should remain
  the same*.
- *Resolve*: releases funds associated with a given disputed transaction. Doesn't require the account to 
  be unlocked.
- *Chargeback*: reverses a disputed transaction, removing the held funds. Doesn't require the account to
  be unlocked.

Transactions can therefore be in any of the following state:

                      ┌────────────┐
                      │            │
             ┌────────►  Applied   │
             │        │            │
             │        └─────┬──────┘
             │              │
             │              │
    Resolve  │              │
             │              │
             │        ┌─────▼───────┐         ┌──────────────┐
             │        │             │         │              │
             └────────┤   Disputed  ├─────────► Charged back │
                      │             │         │              │
                      └─────────────┘         └──────────────┘

Invalid transactions are not applied, but do not cause a break in transaction processing. As it's unclear
how to report such errors in the requirements, they are simply printed to `stderr`.
