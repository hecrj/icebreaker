pub fn open(uri: impl AsRef<str>) {
    let _ = webbrowser::open(uri.as_ref());
}
