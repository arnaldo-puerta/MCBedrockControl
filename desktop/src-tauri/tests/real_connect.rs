//! Reads this machine's real playit state:
//! `cargo test --test real_connect -- --ignored --nocapture`

#[tokio::test]
#[ignore = "reads the real machine"]
async fn prints_this_machines_playit_status() {
    println!("{:#?}", mcbc_lib::connect::playit().await);
}
