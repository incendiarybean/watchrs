use watchrs::WatchRs;

fn main() {
    let watch_dog = WatchRs::default();
    watch_dog
        .begin_watching()
        .expect("Couldn't begin watching.");
}
