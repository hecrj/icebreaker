use crate::core::Url;

pub fn open(url: &Url) {
    let _ = open::that_in_background(url.as_str());
}
