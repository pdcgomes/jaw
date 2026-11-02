use std::os::unix::fs::MetadataExt;

use super::*;

const GB: u64 = 1_000_000_000;
const MB: u64 = 1_000_000;
/// A disk of 500 GB, whose floor is 2 GB.
const SIZE: u64 = 500 * GB;

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
