//! See [`abacus::test::crash`] for documentation.

#![no_main]

use abacus::test::rankings_workload::Workload;
use libfuzzer_sys::fuzz_target;
use tokio;

fuzz_target!(|data: Workload| {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let _ = data.run().await;
    });
});
