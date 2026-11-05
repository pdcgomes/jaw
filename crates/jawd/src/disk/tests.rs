use std::os::unix::fs::MetadataExt;

use super::*;

const GB: u64 = 1_000_000_000;
const MB: u64 = 1_000_000;
/// A disk of 500 GB, whose floor is 2 GB.
const SIZE: u64 = 500 * GB;
/// Free space 100 s of a fill at FILL above the floor and the margin.
const FAR: u64 = 2 * GB + 256 * MB + 100 * FILL;

fn config() -> Config {
    Config { ballast: MB, ..Config::default() }
}

#[test]
fn the_floor_is_2_gb_or_5_percent_of_the_disk_where_thats_less() {
    let config = Config::default();
    assert_eq!(config.floor(SIZE), 2 * GB);
    assert_eq!(config.floor(20 * GB), GB, "5% of a 20 GB disk");
    let unwatched = Disk::unwatched();
    assert!(unwatched.room(), "a disk that isn't watched is never short of room");
    assert_eq!(unwatched.status(), None);
}

#[tokio::test]
async fn below_the_floor_background_work_waits_until_the_disk_has_the_floor_and_the_margin_more() {
    let home = tempfile::tempdir().unwrap();
    let (disk, fake) = Disk::faked(home.path(), config(), 10 * GB, SIZE);
    let mut changes = disk.changes();
    assert!(disk.room());
    assert_eq!(disk.paused(), None);
    fake.free.store(1_900 * MB, Ordering::Relaxed);
    assert!(!disk.room(), "below the floor");
    let why = disk.paused().unwrap();
    assert!(why.contains("jaw's disk has 1.9 GB free, under the 2.0 GB jaw keeps free"), "{why}");
    assert!(changes.has_changed().unwrap(), "it says so as it goes below");
    changes.borrow_and_update();
    fake.free.store(2_100 * MB, Ordering::Relaxed);
    assert!(!disk.room(), "past the floor, but not by the margin");
    fake.free.store(2 * GB + 256 * MB, Ordering::Relaxed);
    assert!(disk.room(), "the floor and the margin more");
    assert_eq!(disk.paused(), None);
    assert!(changes.has_changed().unwrap(), "it says so as it has room again");
    let status = disk.status().unwrap();
    assert_eq!((status.free, status.size, status.floor), (2 * GB + 256 * MB, SIZE, 2 * GB));

    fake.free.store(GB, Ordering::Relaxed);
    let waiting = {
        let disk = disk.clone();
        tokio::spawn(async move { disk.wait_for_room().await })
    };
    tokio::time::sleep(POLL * 2).await;
    assert!(!waiting.is_finished(), "it waits while the disk is short of room");
    fake.free.store(10 * GB, Ordering::Relaxed);
    tokio::time::timeout(POLL * 20, waiting).await.expect("it goes on once there's room").unwrap();
}

#[tokio::test]
async fn while_the_free_space_cant_be_read_what_waits_for_room_does_as_it_did_when_it_last_could() {
    let home = tempfile::tempdir().unwrap();
    let (disk, fake) = Disk::faked(home.path(), config(), GB, SIZE);
    fake.unreadable.store(true, Ordering::Relaxed);
    assert!(disk.room(), "a disk never read has room");
    fake.unreadable.store(false, Ordering::Relaxed);
    assert!(!disk.room());
    fake.unreadable.store(true, Ordering::Relaxed);
    fake.free.store(10 * GB, Ordering::Relaxed);
    assert!(!disk.room(), "short of room when last read, so work still waits");
    let status = disk.status().unwrap();
    assert!(status.paused.is_some(), "as status says: {status:?}");
    assert_eq!(status.free, GB, "the free space last read");
    let waiting = {
        let disk = disk.clone();
        tokio::spawn(async move { disk.wait_for_room().await })
    };
    tokio::time::sleep(POLL * 2).await;
    assert!(!waiting.is_finished(), "it waits while the disk can't be read");
    fake.unreadable.store(false, Ordering::Relaxed);
    tokio::time::timeout(POLL * 20, waiting).await.expect("and goes on once it's read with room").unwrap();
    fake.unreadable.store(true, Ordering::Relaxed);
    fake.free.store(GB, Ordering::Relaxed);
    assert!(disk.room(), "with room when last read, work goes on");
    assert_eq!(disk.paused(), None);
}

#[tokio::test]
async fn the_ballast_is_made_whole_given_up_and_made_again_once_theres_room() {
    let home = tempfile::tempdir().unwrap();
    let (disk, fake) = Disk::faked(home.path(), config(), 10 * GB, SIZE);
    let ballast = home.path().join("ballast");
    disk.tick().await;
    let kept = std::fs::metadata(&ballast).expect("the ballast is made");
    assert_eq!(kept.len(), MB);
    assert!(kept.blocks() * 512 >= MB, "it holds its room: {} blocks", kept.blocks());
    assert!(!home.path().join("ballast.partial").exists(), "it's renamed once whole");
    assert_eq!(fake.free.load(Ordering::Relaxed), 10 * GB - MB, "and takes its room");
    assert_eq!(disk.status().unwrap().ballast, MB);

    disk.full();
    assert!(!ballast.exists(), "a full disk takes it");
    let status = disk.status().unwrap();
    assert_eq!((status.ballast, status.ballast_given_up), (0, 1));
    assert_eq!(fake.free.load(Ordering::Relaxed), 10 * GB, "and has its room back");
    disk.full();
    assert_eq!(disk.status().unwrap().ballast_given_up, 1, "it's given up once");

    fake.free.store(1_900 * MB, Ordering::Relaxed);
    disk.tick().await;
    assert!(!ballast.exists(), "not made while the disk is short of room");
    fake.free.store(2 * GB + 2 * MB, Ordering::Relaxed);
    disk.tick().await;
    assert!(!ballast.exists(), "nor once it has room for it, while it hasn't the margin");
    fake.free.store(10 * GB, Ordering::Relaxed);
    disk.tick().await;
    assert_eq!(std::fs::metadata(&ballast).expect("made again once there's room").len(), MB);
    let status = disk.status().unwrap();
    assert_eq!((status.ballast, status.ballast_given_up), (MB, 1));
}

#[tokio::test]
async fn a_partial_ballast_goes_and_one_of_another_size_is_made_again() {
    let home = tempfile::tempdir().unwrap();
    let (disk, fake) = Disk::faked(home.path(), config(), GB, SIZE);
    std::fs::write(home.path().join("ballast.partial"), [0u8; 10]).unwrap();
    disk.tick().await;
    assert!(
        !home.path().join("ballast.partial").exists(),
        "what a stop left as it was made goes, short of room as it is"
    );
    assert!(!home.path().join("ballast").exists());
    std::fs::write(home.path().join("ballast"), [0u8; 10]).unwrap();
    fake.free.store(10 * GB, Ordering::Relaxed);
    disk.tick().await;
    assert_eq!(std::fs::metadata(home.path().join("ballast")).unwrap().len(), MB, "made again at its size");
}

#[tokio::test]
async fn interfaces_are_told_when_the_disk_goes_below_the_floor_and_when_it_has_room_again() {
    let home = tempfile::tempdir().unwrap();
    let (disk, fake) = Disk::faked(home.path(), config(), 10 * GB, SIZE);
    let (pushes, mut told) = broadcast::channel(8);
    tokio::spawn(tell(disk.changes(), pushes));
    let notice = async |told: &mut broadcast::Receiver<Push>| {
        let push = tokio::time::timeout(Duration::from_secs(5), told.recv()).await.expect("a notice").unwrap();
        assert_eq!(push.method, "notice");
        serde_json::from_value::<Notice>(push.params).unwrap()
    };
    assert!(disk.room());
    fake.free.store(GB, Ordering::Relaxed);
    assert!(!disk.room());
    let low = notice(&mut told).await;
    assert_eq!(low.tone, Tone::Warning);
    assert!(low.text.starts_with("jaw's disk has 1.0 GB free"), "{}", low.text);
    fake.free.store(10 * GB, Ordering::Relaxed);
    assert!(disk.room());
    let back = notice(&mut told).await;
    assert_eq!(back.tone, Tone::Info);
    assert!(back.text.contains("room again"), "{}", back.text);
    assert!(told.try_recv().is_err(), "once each");
}

#[tokio::test]
async fn the_watchdog_looks_again_no_later_than_the_disk_could_come_down_to_the_floor_and_margin() {
    let home = tempfile::tempdir().unwrap();
    // A margin of 10 GB, so a wait that left it out would show.
    let config = Config { margin: 10 * GB, ..config() };
    let above = |seconds: u64| 2 * GB + 10 * GB + seconds * FILL;
    let (disk, fake) = Disk::faked(home.path(), config, above(100) + MB, 4_000 * GB);
    assert_eq!(disk.tick().await, Duration::from_secs(100), "far above the floor, as long as a fill at FILL takes");
    assert_eq!(disk.tick().await, Duration::from_secs(100), "the ballast made in the room set aside for it");
    fake.free.store(above(1_000), Ordering::Relaxed);
    assert_eq!(disk.tick().await, LONGEST, "within LONGEST however far above");
    fake.free.store(above(5), Ordering::Relaxed);
    assert_eq!(disk.tick().await, EVERY, "near the floor and margin, every EVERY");
    fake.free.store(above(0) - MB, Ordering::Relaxed);
    assert_eq!(disk.tick().await, EVERY, "above the floor, under the margin");
    fake.free.store(GB, Ordering::Relaxed);
    assert_eq!(disk.tick().await, EVERY, "below the floor");
    fake.free.store(5 * GB, Ordering::Relaxed);
    assert_eq!(disk.tick().await, EVERY, "past the floor, short of room until it has the margin");
    fake.free.store(above(100), Ordering::Relaxed);
    assert_eq!(disk.tick().await, Duration::from_secs(100), "and as far off as before once it has room");
}

#[tokio::test]
async fn while_its_ballast_isnt_kept_the_watchdog_looks_every_ten_seconds() {
    let home = tempfile::tempdir().unwrap();
    let ballast = home.path().join("ballast");
    let (disk, fake) = Disk::faked(home.path(), config(), FAR + MB, SIZE);
    // As a disk on which it can't be made.
    std::fs::create_dir(home.path().join("ballast.partial")).unwrap();
    assert_eq!(disk.tick().await, EVERY, "far above the floor, with a ballast it can't make");
    assert!(!ballast.exists());
    std::fs::remove_dir(home.path().join("ballast.partial")).unwrap();
    assert_eq!(disk.tick().await, Duration::from_secs(100), "once it's made, as late as the disk's room allows");
    disk.full();
    fake.free.store(2 * GB + MB / 2, Ordering::Relaxed);
    assert_eq!(disk.tick().await, EVERY, "given up, with no room for it beside the floor");
    assert!(!ballast.exists());
    fake.free.store(FAR + MB, Ordering::Relaxed);
    assert_eq!(disk.tick().await, Duration::from_secs(100), "and made again once there's room");
    assert!(ballast.exists());
}

#[tokio::test]
async fn while_the_free_space_cant_be_read_the_watchdog_looks_every_ten_seconds() {
    let home = tempfile::tempdir().unwrap();
    let (disk, fake) = Disk::faked(home.path(), config(), FAR + MB, SIZE);
    fake.unreadable.store(true, Ordering::Relaxed);
    assert_eq!(disk.tick().await, EVERY, "never read");
    fake.unreadable.store(false, Ordering::Relaxed);
    assert_eq!(disk.tick().await, Duration::from_secs(100));
    fake.unreadable.store(true, Ordering::Relaxed);
    assert_eq!(disk.tick().await, EVERY, "read far above the floor, but not now");
}

#[tokio::test(start_paused = true)]
async fn far_above_the_floor_the_watchdog_looks_only_as_often_as_the_disk_could_come_down_to_it() {
    let home = tempfile::tempdir().unwrap();
    let (disk, fake) = Disk::faked(home.path(), config(), FAR + MB, SIZE);
    disk.start();
    tokio::time::sleep(Duration::from_secs(650)).await;
    assert_eq!(fake.reads.load(Ordering::Relaxed), 7, "every 100 s from the start, not every EVERY");
}

#[tokio::test(start_paused = true)]
async fn a_disk_filling_at_up_to_fill_bytes_a_second_is_told_below_the_floor_within_ten_seconds_of_it() {
    for rate in [FILL, FILL / 4] {
        let home = tempfile::tempdir().unwrap();
        let (disk, fake) = Disk::faked(home.path(), config(), FAR + MB, SIZE);
        let (pushes, mut told) = broadcast::channel(8);
        tokio::spawn(tell(disk.changes(), pushes));
        disk.start();
        let began = tokio::time::Instant::now();
        // Something other than jaw fills the disk while jaw is quiet, a second's worth at a time.
        let below = Arc::new(std::sync::Mutex::new(None));
        let filling = tokio::spawn({
            let (free, below) = (fake.free.clone(), below.clone());
            async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let left = free.fetch_sub(rate.min(free.load(Ordering::Relaxed)), Ordering::Relaxed);
                    if left.saturating_sub(rate) < 2 * GB {
                        below.lock().unwrap().get_or_insert_with(tokio::time::Instant::now);
                    }
                }
            }
        });
        let push = tokio::time::timeout(Duration::from_secs(3_600), told.recv()).await.expect("told").unwrap();
        let told_at = began.elapsed();
        let went_below = below.lock().unwrap().expect("below the floor before it's told") - began;
        filling.abort();
        let notice = serde_json::from_value::<Notice>(push.params).unwrap();
        assert_eq!(notice.tone, Tone::Warning, "{}", notice.text);
        assert!(
            told_at <= went_below + EVERY,
            "filled at {rate} B a second: below the floor at {went_below:?}, told at {told_at:?}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_ballast_given_up_is_made_again_within_ten_seconds_of_the_disk_having_room_however_far_off_the_look_was() {
    let home = tempfile::tempdir().unwrap();
    let ballast = home.path().join("ballast");
    let (disk, fake) = Disk::faked(home.path(), config(), FAR + MB, SIZE);
    disk.start();
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(ballast.exists(), "the first look makes it, and the next is 100 s off");
    // Something fills the disk faster than FILL, and a write to jaw.db is refused.
    fake.free.store(0, Ordering::Relaxed);
    disk.full();
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(disk.paused().is_some(), "the watchdog looked at once, finding the disk short of room");
    tokio::time::sleep(Duration::from_secs(40)).await;
    assert!(!ballast.exists(), "not made again while the disk is short of room");
    fake.free.store(FAR, Ordering::Relaxed);
    tokio::time::sleep(EVERY + Duration::from_secs(1)).await;
    assert!(ballast.exists(), "made again within EVERY of the disk having room");
}

#[tokio::test(start_paused = true)]
async fn below_the_floor_what_waits_for_room_waits_at_once_however_far_off_the_watchdogs_next_look_is() {
    let home = tempfile::tempdir().unwrap();
    let (disk, fake) = Disk::faked(home.path(), config(), FAR + MB, SIZE);
    let mut changes = disk.changes();
    disk.start();
    tokio::time::sleep(Duration::from_secs(1)).await;
    changes.borrow_and_update();
    fake.free.store(GB, Ordering::Relaxed);
    assert!(!disk.room(), "a background transaction, an indexing step, a download or a history blob reads the disk");
    assert!(changes.has_changed().unwrap(), "and interfaces are told as it does");
    let waiting = {
        let disk = disk.clone();
        tokio::spawn(async move { disk.wait_for_room().await })
    };
    tokio::time::sleep(POLL * 4).await;
    assert!(!waiting.is_finished(), "it waits while the disk is short of room");
    fake.free.store(FAR, Ordering::Relaxed);
    tokio::time::timeout(POLL * 4, waiting).await.expect("and goes on once there's room, the watchdog asleep").unwrap();
}
