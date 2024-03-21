use file_path_from_id::path_from_id;
use notify::{RecursiveMode, Watcher};
use notify_debouncer_full::{DebounceEventResult, FileIdCache};
use std::fs;
use std::sync::mpsc;
use std::time::Duration;

const WATCH_PATH: &'static str = "watched";

/// # Notes
/// + Must be run from the `examples/basic` folder due to relative paths.
fn main() {
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify_debouncer_full::new_debouncer(
        Duration::from_millis(200),
        None,
        move |event: DebounceEventResult| {
            tx.send(event).unwrap();
        },
    )
    .unwrap();

    let watch_path = fs::canonicalize(WATCH_PATH).unwrap();
    watcher
        .watcher()
        .watch(&watch_path, RecursiveMode::Recursive)
        .unwrap();

    watcher
        .cache()
        .add_root(&watch_path, RecursiveMode::Recursive);

    loop {
        match rx.recv().unwrap() {
            Ok(events) => {
                for event in events {
                    let cache = watcher.cache();

                    println!("{cache:?}\n");
                    println!("{event:?}\n");
                    match &event.paths[..] {
                        [path] => {
                            let Some(info) = cache.cached_file_id(&path) else {
                                println!("path not cached\n");
                                continue;
                            };

                            match path_from_id(info) {
                                Ok(path) => println!("found {path:?}\n"),
                                Err(err) => println!("err {err:?}\n"),
                            }
                        }

                        [from, to] => {
                            match cache.cached_file_id(&from) {
                                None => {
                                    println!("from path not cached\n");
                                }

                                Some(info) => match path_from_id(info) {
                                    Ok(path) => println!("from {path:?}\n"),
                                    Err(err) => println!("from err {err:?}\n"),
                                },
                            }

                            match cache.cached_file_id(&to) {
                                None => {
                                    println!("to path not cached\n");
                                }

                                Some(info) => match path_from_id(info) {
                                    Ok(path) => println!("to {path:?}\n"),
                                    Err(err) => println!("to err {err:?}\n"),
                                },
                            }
                        }

                        _ => {}
                    }
                }
            }

            Err(err) => println!("ERR {err:?}\n"),
        }
    }
}
