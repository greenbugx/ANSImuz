use ansimuz::discord::{DiscordClient, DiscordWorker, PlaybackInfo, build_activity};
use presenceforge::ActivityBuilder;
use presenceforge::sync::DiscordIpcClient;
use std::thread::sleep;
use std::time::Duration;

const TEST_CLIENT_ID: &str = ansimuz::discord::DEFAULT_CLIENT_ID;

#[test]
fn test_discord_presenceforge_direct_lifecycle() {
    println!("Initializing direct DiscordIpcClient with ID: {TEST_CLIENT_ID}");

    let mut client = match DiscordIpcClient::new(TEST_CLIENT_ID) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create Discord client: {e:?}");
            return;
        }
    };

    // Connect to Discord
    println!("Connecting to Discord IPC...");
    match client.connect() {
        Ok(ready_event) => {
            println!("Connected to Discord! Ready event: {ready_event:?}")
        }
        Err(e) => {
            println!("Discord is not reachable or not running ({e:?}). Skipping live test.");
            return;
        }
    }

    assert!(client.is_connected(), "Client should report connected");

    // Set a basic activity
    println!("Setting basic activity...");
    let basic_activity = ActivityBuilder::new()
        .state("Listening to ANSImuz")
        .details("Track 1 - Initial Track")
        .build();

    let set_res = client.set_activity(&basic_activity);
    println!("set_activity result: {set_res:?}");
    assert!(set_res.is_ok(), "Setting basic activity should succeed");

    sleep(Duration::from_millis(500));

    // Update the activity
    println!("Updating activity...");
    let updated_activity = ActivityBuilder::new()
        .state("Listening to ANSImuz")
        .details("Track 2 - Updated Track")
        .build();

    let update_res = client.set_activity(&updated_activity);
    println!("update_activity result: {update_res:?}");
    assert!(update_res.is_ok(), "Updating activity should succeed");

    sleep(Duration::from_millis(500));

    // Clear the activity
    println!("Clearing activity...");
    let clear_res = client.clear_activity();
    println!("clear_activity result: {clear_res:?}");
    assert!(clear_res.is_ok(), "Clearing activity should succeed");

    sleep(Duration::from_millis(200));

    // Disconnect cleanly
    println!("Closing connection...");
    client.close();
    assert!(!client.is_connected(), "Client should report disconnected");

    println!("Direct PresenceForge test completed cleanly!");
}

#[test]
fn test_discord_client_abstraction() {
    let mut client = DiscordClient::new(TEST_CLIENT_ID);
    assert!(!client.is_connected());

    // Connect
    if client.connect().is_err() {
        println!("Discord client connect failed or Discord not running. Skipping.");
        return;
    }
    assert!(client.is_connected());

    // Set activity from PlaybackInfo
    let track1 = PlaybackInfo::new("Track 1", "Artist 1")
        .with_album("Album 1")
        .with_position(Duration::from_secs(10))
        .with_duration(Duration::from_secs(200));
    let activity1 = build_activity(&track1);
    assert!(client.set_activity(&activity1).is_ok());

    sleep(Duration::from_millis(300));

    // Update activity
    let track2 = PlaybackInfo::new("Track 2", "Artist 2").with_paused(true);
    let activity2 = build_activity(&track2);
    assert!(client.set_activity(&activity2).is_ok());

    sleep(Duration::from_millis(300));

    // Clear activity
    assert!(client.clear_activity().is_ok());

    // Disconnect cleanly
    client.disconnect();
    assert!(!client.is_connected());
}

#[test]
fn test_discord_worker_thread_non_blocking() {
    let mut worker = DiscordWorker::spawn(TEST_CLIENT_ID).expect("spawn worker");

    // Send commands non-blocking
    let track = PlaybackInfo::new("Bohemian Rhapsody", "Queen")
        .with_album("A Night at the Opera")
        .with_position(Duration::from_secs(45))
        .with_duration(Duration::from_secs(354));

    worker.track_changed(track, None);
    sleep(Duration::from_millis(50));

    worker.position_changed(Duration::from_secs(50));
    sleep(Duration::from_millis(50));

    worker.paused();
    sleep(Duration::from_millis(50));

    worker.resumed(Duration::from_secs(50));
    sleep(Duration::from_millis(50));

    worker.clear();
    sleep(Duration::from_millis(50));

    // Shutdown worker cleanly
    worker.shutdown();
}
