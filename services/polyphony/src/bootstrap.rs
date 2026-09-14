use crate::{reducers, sources, storage};

use gasket::{messaging::tokio::connect_ports, runtime::Tether};

pub struct Pipeline {
    pub tethers: Vec<Tether>,
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl Pipeline {
    pub fn new() -> Self {
        Self {
            tethers: Vec::new(),
        }
    }

    pub fn register_stage(&mut self, tether: Tether) {
        self.tethers.push(tether);
    }
}

pub async fn build(
    mut source: sources::Bootstrapper,
    mut reducer: reducers::Bootstrapper,
    mut storage: storage::Bootstrapper,
) -> Result<Pipeline, crate::Error> {
    let mut cursor = storage.build_cursor();
    let persistent_buf = cursor.fetch_persistent_buffer().await?;

    let last_point = cursor.last_point().await?;

    let mut pipeline = Pipeline::new();

    connect_ports(
        source.borrow_output_port(),
        reducer.borrow_input_port(),
        1000,
    );

    connect_ports(
        reducer.borrow_output_port(),
        storage.borrow_input_port(),
        1000,
    );

    source.spawn_stages(&mut pipeline, cursor);
    reducer.spawn_stages(&mut pipeline);
    storage.spawn_stages(&mut pipeline, last_point, persistent_buf);

    Ok(pipeline)
}
