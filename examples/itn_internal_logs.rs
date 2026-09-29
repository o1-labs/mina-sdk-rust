//! Read a daemon's internal logs through its ITN GraphQL server.
//!
//! ```sh
//! MINA_ITN_URI=http://127.0.0.1:3086/graphql MINA_ITN_KEY=<base64 seed> \
//!   cargo run --example itn_internal_logs --features itn
//! ```

use mina_sdk::itn::{ItnClient, ItnKey};

#[tokio::main]
async fn main() -> mina_sdk::Result<()> {
    let uri = std::env::var("MINA_ITN_URI").unwrap_or("http://127.0.0.1:3086/graphql".into());
    let key = ItnKey::from_base64(&std::env::var("MINA_ITN_KEY").expect("set MINA_ITN_KEY"))?;
    let client = ItnClient::new(&uri, key);

    let auth = client.auth().await?;
    println!(
        "server {} (block producer: {}), next sequence number {}",
        auth.server_uuid, auth.is_block_producer, auth.signer_sequence_number
    );

    let logs = client.internal_logs(0).await?;
    println!("{} internal logs", logs.len());
    for log in logs.iter().take(3) {
        println!(
            "  #{} {} {} ({} metadata)",
            log.id,
            log.timestamp,
            log.message,
            log.metadata.len()
        );
    }
    Ok(())
}
