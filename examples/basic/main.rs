use file_path_from_id::path_from_id;
use notify::{self, RecursiveMode, Watcher};
use notify_debouncer_full::{DebounceEventResult, FileIdCache};
use std::fs;
use std::sync::mpsc;
use std::time::Duration;

const WATCH_PATH: &'static str = "watched";

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
                                println!("path not cached");
                                continue;
                            };

                            match path_from_id(info) {
                                Ok(path) => println!("found {path:?}"),
                                Err(err) => println!("err {err:?}"),
                            }
                        }

                        [from, to] => {
                            match cache.cached_file_id(&from) {
                                None => {
                                    println!("from path not cached");
                                }

                                Some(info) => match path_from_id(info) {
                                    Ok(path) => println!("from {path:?}"),
                                    Err(err) => println!("from err {err:?}"),
                                },
                            }

                            match cache.cached_file_id(&to) {
                                None => {
                                    println!("to path not cached");
                                }

                                Some(info) => match path_from_id(info) {
                                    Ok(path) => println!("to {path:?}"),
                                    Err(err) => println!("to err {err:?}"),
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
