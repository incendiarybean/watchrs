use watchrs::WatchRs;

fn main() {
    WatchRs::default()
        .begin_watching()
        .expect("Couldn't begin watching.");
}
