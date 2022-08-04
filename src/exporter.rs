use anyhow::{Context, Result};
use csv::Writer;
use std::io::Write;

use crate::model::ClientState;

/// Abstract client state exporter.
pub trait ClientStateExporter {
    /// Serializes the give client state to its intended destination.
    fn serialize(&mut self, client_state: &ClientState) -> Result<()>;
}

impl<W: Write> ClientStateExporter for Writer<W> {
    fn serialize(&mut self, client_state: &ClientState) -> Result<()> {
        // call our writer version of serialize
        Writer::serialize(self, client_state).with_context(|| {
            format!(
                "Error serializing state for client: {}",
                client_state.client_id()
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use csv::Writer;
    use rust_decimal::Decimal;

    use crate::model::{ClientId, ClientState};

    #[test]
    fn should_serialize_state_to_csv() {
        let mut state = ClientState::new(ClientId::new(2));
        state.deposit(Decimal::from(3)).unwrap();

        let mut writer = Writer::from_writer(vec![]);
        writer.serialize(&state).unwrap();

        let data = String::from_utf8(writer.into_inner().unwrap()).unwrap();
        assert_eq!(
            data,
            "client,available,held,total,locked\n2,3.0000,0.0000,3.0000,false\n"
        )
    }
}
