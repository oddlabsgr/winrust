mod common;

use common::{fixture, full_tags, playlist, write_tags};
use crossbeam_channel::{RecvTimeoutError, unbounded};
use std::path::PathBuf;
use std::time::Duration;
use winrust_playlist::entry::EntryId;
use winrust_playlist::tags::FileTags;
use winrust_playlist::worker::{TagResult, TagWorker};

const WAIT: Duration = Duration::from_secs(10);

fn ids(n: usize) -> Vec<EntryId> {
    playlist(n).entries().iter().map(|e| e.id()).collect()
}

#[test]
fn the_worker_reads_tags_and_reports_them_by_id() {
    let dir = tempfile::tempdir().unwrap();
    let tagged = fixture(dir.path(), "tone.mp3");
    write_tags(&tagged, &full_tags());
    let id = ids(2);
    let (tx, rx) = unbounded();
    let worker = TagWorker::new(move |id, result| tx.send((id, result)).unwrap());
    worker.request(vec![
        (id[0], tagged),
        (id[1], dir.path().join("missing.mp3")),
    ]);
    let (first, result) = rx.recv_timeout(WAIT).unwrap();
    assert_eq!(first, id[0]);
    assert_eq!(result.unwrap().tags, full_tags());
    let (second, result) = rx.recv_timeout(WAIT).unwrap();
    assert_eq!(second, id[1]);
    assert!(result.is_err());
}

#[test]
fn a_new_request_replaces_the_ones_still_waiting() {
    let id = ids(5);
    let (started_tx, started) = unbounded::<()>();
    let (release, gate) = unbounded::<()>();
    let read = move |path: &std::path::Path| -> TagResult {
        if path.ends_with("slow") {
            started_tx.send(()).unwrap();
            gate.recv_timeout(WAIT).unwrap();
        }
        Ok(FileTags::default())
    };
    let (tx, rx) = unbounded();
    let worker = TagWorker::with_reader(read, move |id, _| tx.send(id).unwrap());
    let path = |name: &str| PathBuf::from(name);
    worker.request(vec![
        (id[0], path("slow")),
        (id[1], path("a")),
        (id[2], path("b")),
    ]);
    started.recv_timeout(WAIT).unwrap();
    worker.request(vec![(id[3], path("c")), (id[4], path("d"))]);
    release.send(()).unwrap();
    let got: Vec<EntryId> = (0..3).map(|_| rx.recv_timeout(WAIT).unwrap()).collect();
    assert_eq!(got, [id[0], id[3], id[4]], "the read under way finishes");
    assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn dropping_the_worker_stops_its_thread() {
    let (tx, rx) = unbounded::<EntryId>();
    let worker = TagWorker::new(move |id, _| tx.send(id).unwrap());
    drop(worker);
    // The thread owned `tx`; once it ends, the channel disconnects.
    assert_eq!(rx.recv_timeout(WAIT), Err(RecvTimeoutError::Disconnected));
}

#[test]
fn a_file_that_makes_the_reader_panic_does_not_stop_the_worker() {
    let id = ids(2);
    let read = |path: &std::path::Path| -> TagResult {
        if path.ends_with("bad") {
            panic!("the tag library choked on this file");
        }
        Ok(FileTags::default())
    };
    let (tx, rx) = unbounded();
    let worker = TagWorker::with_reader(read, move |id, r: TagResult| {
        tx.send((id, r.is_ok())).unwrap()
    });
    worker.request(vec![(id[0], PathBuf::from("bad"))]);
    assert_eq!(rx.recv_timeout(WAIT), Ok((id[0], false)));
    worker.request(vec![(id[1], PathBuf::from("good"))]);
    assert_eq!(rx.recv_timeout(WAIT), Ok((id[1], true)));
}
