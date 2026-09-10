fn main() {
    if seeklocal_desktop::run().is_err() {
        eprintln!("SeekLocal could not start");
    }
}
